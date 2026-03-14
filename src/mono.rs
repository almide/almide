/// Monomorphization pass: IR → IR transform.
///
/// Specializes generic functions with structural bounds (e.g., `T: { name: String, .. }`)
/// into concrete versions for each call-site type. This enables Rust codegen
/// to emit functions that preserve the full concrete type.
///
/// Example:
///   fn set_name[T: { name: String, .. }](x: T, n: String) -> T
///   set_name(dog, "Max")     → set_name__Dog(x: Dog, n: String) -> Dog
///   set_name(person, "Bob")  → set_name__Person(x: Person, n: String) -> Person

use std::collections::HashMap;
use crate::ir::*;
use crate::types::Ty;

/// Key for a monomorphized instance: (function_name, concrete_type_suffix).
type MonoKey = (String, String);

/// Run the monomorphization pass on an IR program.
/// Only affects functions with structurally-bounded type parameters.
pub fn monomorphize(program: &mut IrProgram) {
    // Step 1: Identify functions with structural bounds on generics
    let bound_fns = find_structurally_bounded_fns(&program.functions);
    if bound_fns.is_empty() {
        return;
    }

    // Step 2: Discover instantiations by scanning all call sites
    let instances = discover_instances(program, &bound_fns);
    if instances.is_empty() {
        return;
    }

    // Step 3: Clone and specialize functions
    let mut new_functions = Vec::new();
    for ((fn_name, suffix), bindings) in &instances {
        if let Some(orig) = program.functions.iter().find(|f| f.name == *fn_name) {
            let specialized = specialize_function(orig, suffix, bindings);
            new_functions.push(specialized);
        }
    }

    // Step 4: Rewrite call sites
    rewrite_calls(program, &bound_fns, &instances);

    // Step 5: Add specialized functions to the program
    program.functions.extend(new_functions);
}

/// Info about a structurally-bounded type parameter in a function.
struct BoundedParam {
    /// Index of the parameter in the function signature
    param_idx: usize,
    /// Name of the type variable (e.g., "T")
    type_var: String,
}

/// Find functions that have structural bounds on generic type parameters.
/// Returns function_name → list of bounded params.
fn find_structurally_bounded_fns(functions: &[IrFunction]) -> HashMap<String, Vec<BoundedParam>> {
    let mut result = HashMap::new();
    for func in functions {
        let mut bounded = Vec::new();
        if let Some(ref generics) = func.generics {
            for g in generics {
                if g.structural_bound.is_some() {
                    // Find which params use this type variable
                    // In IR, TypeVar("T") may appear as Named("T", []) depending on lowering
                    for (i, param) in func.params.iter().enumerate() {
                        let is_match = matches!(&param.ty, Ty::TypeVar(n) if n == &g.name)
                            || matches!(&param.ty, Ty::Named(n, args) if n == &g.name && args.is_empty());
                        if is_match {
                            bounded.push(BoundedParam {
                                param_idx: i,
                                type_var: g.name.clone(),
                            });
                        }
                    }
                }
            }
        }
        if !bounded.is_empty() {
            result.insert(func.name.clone(), bounded);
        }
    }
    result
}

/// Discover all concrete instantiations of structurally-bounded functions.
fn discover_instances(
    program: &IrProgram,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
) -> HashMap<MonoKey, HashMap<String, Ty>> {
    let mut instances: HashMap<MonoKey, HashMap<String, Ty>> = HashMap::new();

    for func in &program.functions {
        discover_in_expr(&func.body, bound_fns, &mut instances);
    }
    for tl in &program.top_lets {
        discover_in_expr(&tl.value, bound_fns, &mut instances);
    }

    instances
}

fn discover_in_expr(
    expr: &IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get(name) {
                    let mut bindings: HashMap<String, Ty> = HashMap::new();
                    for bp in bounded_params {
                        if bp.param_idx < args.len() {
                            bindings.insert(bp.type_var.clone(), args[bp.param_idx].ty.clone());
                        }
                    }
                    if !bindings.is_empty() {
                        let suffix = mangle_suffix(&bindings);
                        instances.insert((name.clone(), suffix), bindings);
                    }
                }
            }
            for arg in args { discover_in_expr(arg, bound_fns, instances); }
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    discover_in_expr(object, bound_fns, instances);
                }
                _ => {}
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            discover_in_expr(left, bound_fns, instances);
            discover_in_expr(right, bound_fns, instances);
        }
        IrExprKind::UnOp { operand, .. } => discover_in_expr(operand, bound_fns, instances),
        IrExprKind::If { cond, then, else_ } => {
            discover_in_expr(cond, bound_fns, instances);
            discover_in_expr(then, bound_fns, instances);
            discover_in_expr(else_, bound_fns, instances);
        }
        IrExprKind::Match { subject, arms } => {
            discover_in_expr(subject, bound_fns, instances);
            for arm in arms {
                if let Some(g) = &arm.guard { discover_in_expr(g, bound_fns, instances); }
                discover_in_expr(&arm.body, bound_fns, instances);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { discover_in_stmt(s, bound_fns, instances); }
            if let Some(e) = expr { discover_in_expr(e, bound_fns, instances); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            discover_in_expr(iterable, bound_fns, instances);
            for s in body { discover_in_stmt(s, bound_fns, instances); }
        }
        IrExprKind::While { cond, body } => {
            discover_in_expr(cond, bound_fns, instances);
            for s in body { discover_in_stmt(s, bound_fns, instances); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { discover_in_expr(e, bound_fns, instances); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { discover_in_expr(e, bound_fns, instances); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            discover_in_expr(base, bound_fns, instances);
            for (_, e) in fields { discover_in_expr(e, bound_fns, instances); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                discover_in_expr(k, bound_fns, instances);
                discover_in_expr(v, bound_fns, instances);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            discover_in_expr(start, bound_fns, instances);
            discover_in_expr(end, bound_fns, instances);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            discover_in_expr(object, bound_fns, instances);
        }
        IrExprKind::IndexAccess { object, index } => {
            discover_in_expr(object, bound_fns, instances);
            discover_in_expr(index, bound_fns, instances);
        }
        IrExprKind::Lambda { body, .. } => discover_in_expr(body, bound_fns, instances),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    discover_in_expr(expr, bound_fns, instances);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => discover_in_expr(expr, bound_fns, instances),
        _ => {}
    }
}

fn discover_in_stmt(
    stmt: &IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => discover_in_expr(value, bound_fns, instances),
        IrStmtKind::IndexAssign { index, value, .. } => {
            discover_in_expr(index, bound_fns, instances);
            discover_in_expr(value, bound_fns, instances);
        }
        IrStmtKind::FieldAssign { value, .. } => discover_in_expr(value, bound_fns, instances),
        IrStmtKind::Expr { expr } => discover_in_expr(expr, bound_fns, instances),
        IrStmtKind::Guard { cond, else_ } => {
            discover_in_expr(cond, bound_fns, instances);
            discover_in_expr(else_, bound_fns, instances);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Generate a mangled suffix from type variable bindings.
fn mangle_suffix(bindings: &HashMap<String, Ty>) -> String {
    let mut parts: Vec<String> = bindings.iter().map(|(_, ty)| mangle_ty(ty)).collect();
    parts.sort();
    parts.join("_")
}

fn mangle_ty(ty: &Ty) -> String {
    match ty {
        Ty::Named(name, args) => {
            if args.is_empty() { name.clone() }
            else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
        Ty::Record { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            names.join("_")
        }
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::List(inner) => format!("List_{}", mangle_ty(inner)),
        _ => "Unknown".into(),
    }
}

/// Clone and specialize a function for concrete types.
fn specialize_function(
    orig: &IrFunction,
    suffix: &str,
    bindings: &HashMap<String, Ty>,
) -> IrFunction {
    let mut func = orig.clone();
    func.name = format!("{}__{}", orig.name, suffix);

    // Remove structural bounds from generics (specialized function is concrete)
    func.generics = None;

    // Substitute type variables in parameter types
    for param in &mut func.params {
        param.ty = substitute_ty(&param.ty, bindings);
    }
    func.ret_ty = substitute_ty(&func.ret_ty, bindings);
    substitute_expr_types(&mut func.body, bindings);

    func
}

/// Substitute TypeVars with concrete types.
fn substitute_ty(ty: &Ty, bindings: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        // In IR, TypeVar("T") may appear as Named("T", [])
        Ty::Named(name, args) if args.is_empty() && bindings.contains_key(name) => {
            bindings[name].clone()
        }
        Ty::List(inner) => Ty::List(Box::new(substitute_ty(inner, bindings))),
        Ty::Option(inner) => Ty::Option(Box::new(substitute_ty(inner, bindings))),
        Ty::Result(ok, err) => Ty::Result(Box::new(substitute_ty(ok, bindings)), Box::new(substitute_ty(err, bindings))),
        Ty::Map(k, v) => Ty::Map(Box::new(substitute_ty(k, bindings)), Box::new(substitute_ty(v, bindings))),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| substitute_ty(p, bindings)).collect(),
            ret: Box::new(substitute_ty(ret, bindings)),
        },
        Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| substitute_ty(t, bindings)).collect()),
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute_ty(t, bindings))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (n.clone(), substitute_ty(t, bindings))).collect(),
        },
        _ => ty.clone(),
    }
}

fn substitute_expr_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
    expr.ty = substitute_ty(&expr.ty, bindings);
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            substitute_expr_types(left, bindings);
            substitute_expr_types(right, bindings);
        }
        IrExprKind::UnOp { operand, .. } => substitute_expr_types(operand, bindings),
        IrExprKind::If { cond, then, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(then, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrExprKind::Match { subject, arms } => {
            substitute_expr_types(subject, bindings);
            for arm in arms {
                if let Some(g) = &mut arm.guard { substitute_expr_types(g, bindings); }
                substitute_expr_types(&mut arm.body, bindings);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { substitute_stmt_types(s, bindings); }
            if let Some(e) = expr { substitute_expr_types(e, bindings); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    substitute_expr_types(object, bindings);
                }
                _ => {}
            }
            for a in args { substitute_expr_types(a, bindings); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { substitute_expr_types(e, bindings); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { substitute_expr_types(e, bindings); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            substitute_expr_types(base, bindings);
            for (_, e) in fields { substitute_expr_types(e, bindings); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                substitute_expr_types(k, bindings);
                substitute_expr_types(v, bindings);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            substitute_expr_types(start, bindings);
            substitute_expr_types(end, bindings);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            substitute_expr_types(object, bindings);
        }
        IrExprKind::IndexAccess { object, index } => {
            substitute_expr_types(object, bindings);
            substitute_expr_types(index, bindings);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            substitute_expr_types(iterable, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        IrExprKind::While { cond, body } => {
            substitute_expr_types(cond, bindings);
            for s in body { substitute_stmt_types(s, bindings); }
        }
        IrExprKind::Lambda { body, params, .. } => {
            for (_, ty) in params { *ty = substitute_ty(ty, bindings); }
            substitute_expr_types(body, bindings);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    substitute_expr_types(expr, bindings);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => substitute_expr_types(expr, bindings),
        _ => {}
    }
}

fn substitute_stmt_types(stmt: &mut IrStmt, bindings: &HashMap<String, Ty>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, ty, .. } => {
            *ty = substitute_ty(ty, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Assign { value, .. } => {
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            substitute_expr_types(index, bindings);
            substitute_expr_types(value, bindings);
        }
        IrStmtKind::FieldAssign { value, .. } => substitute_expr_types(value, bindings),
        IrStmtKind::Expr { expr } => substitute_expr_types(expr, bindings),
        IrStmtKind::Guard { cond, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Rewrite call sites to point to specialized functions.
fn rewrite_calls(
    program: &mut IrProgram,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
) {
    for func in &mut program.functions {
        rewrite_expr_calls(&mut func.body, bound_fns, instances);
    }
    for tl in &mut program.top_lets {
        rewrite_expr_calls(&mut tl.value, bound_fns, instances);
    }
}

fn rewrite_expr_calls(
    expr: &mut IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &mut expr.kind {
        IrExprKind::Call { target, args, .. } => {
            for a in args.iter_mut() { rewrite_expr_calls(a, bound_fns, instances); }
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get(name.as_str()) {
                    let mut bindings: HashMap<String, Ty> = HashMap::new();
                    for bp in bounded_params {
                        if bp.param_idx < args.len() {
                            bindings.insert(bp.type_var.clone(), args[bp.param_idx].ty.clone());
                        }
                    }
                    if !bindings.is_empty() {
                        let suffix = mangle_suffix(&bindings);
                        if instances.contains_key(&(name.clone(), suffix.clone())) {
                            *name = format!("{}__{}", name, suffix);
                        }
                    }
                }
            }
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    rewrite_expr_calls(object, bound_fns, instances);
                }
                _ => {}
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            rewrite_expr_calls(left, bound_fns, instances);
            rewrite_expr_calls(right, bound_fns, instances);
        }
        IrExprKind::UnOp { operand, .. } => rewrite_expr_calls(operand, bound_fns, instances),
        IrExprKind::If { cond, then, else_ } => {
            rewrite_expr_calls(cond, bound_fns, instances);
            rewrite_expr_calls(then, bound_fns, instances);
            rewrite_expr_calls(else_, bound_fns, instances);
        }
        IrExprKind::Match { subject, arms } => {
            rewrite_expr_calls(subject, bound_fns, instances);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_expr_calls(g, bound_fns, instances); }
                rewrite_expr_calls(&mut arm.body, bound_fns, instances);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { rewrite_stmt_calls(s, bound_fns, instances); }
            if let Some(e) = expr { rewrite_expr_calls(e, bound_fns, instances); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            rewrite_expr_calls(iterable, bound_fns, instances);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances); }
        }
        IrExprKind::While { cond, body } => {
            rewrite_expr_calls(cond, bound_fns, instances);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { rewrite_expr_calls(e, bound_fns, instances); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            rewrite_expr_calls(base, bound_fns, instances);
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                rewrite_expr_calls(k, bound_fns, instances);
                rewrite_expr_calls(v, bound_fns, instances);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            rewrite_expr_calls(start, bound_fns, instances);
            rewrite_expr_calls(end, bound_fns, instances);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            rewrite_expr_calls(object, bound_fns, instances);
        }
        IrExprKind::IndexAccess { object, index } => {
            rewrite_expr_calls(object, bound_fns, instances);
            rewrite_expr_calls(index, bound_fns, instances);
        }
        IrExprKind::Lambda { body, .. } => rewrite_expr_calls(body, bound_fns, instances),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    rewrite_expr_calls(expr, bound_fns, instances);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => rewrite_expr_calls(expr, bound_fns, instances),
        _ => {}
    }
}

fn rewrite_stmt_calls(
    stmt: &mut IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => rewrite_expr_calls(value, bound_fns, instances),
        IrStmtKind::IndexAssign { index, value, .. } => {
            rewrite_expr_calls(index, bound_fns, instances);
            rewrite_expr_calls(value, bound_fns, instances);
        }
        IrStmtKind::FieldAssign { value, .. } => rewrite_expr_calls(value, bound_fns, instances),
        IrStmtKind::Expr { expr } => rewrite_expr_calls(expr, bound_fns, instances),
        IrStmtKind::Guard { cond, else_ } => {
            rewrite_expr_calls(cond, bound_fns, instances);
            rewrite_expr_calls(else_, bound_fns, instances);
        }
        IrStmtKind::Comment { .. } => {}
    }
}
