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

use std::collections::HashMap;
use crate::ir::*;
use crate::types::Ty;

/// Key for a monomorphized instance: (function_name, concrete_type_suffix).
type MonoKey = (String, String);

/// Run the monomorphization pass on an IR program.
/// Only affects functions with structurally-bounded type parameters.
pub fn monomorphize(program: &mut IrProgram) {
    // Step 1: Identify functions with structural bounds on generics
    let bound_fns = find_structurally_bounded_fns(&program.functions, &program.type_decls);
    if bound_fns.is_empty() {
        return;
    }

    // Fixed-point loop: transitive monomorphization (A → B → C chains)
    let mut all_instances: HashMap<MonoKey, HashMap<String, Ty>> = HashMap::new();
    let max_iterations = 10;
    for _ in 0..max_iterations {
        // Discover new instantiations (includes scanning previously generated specialized functions)
        let instances = discover_instances(program, &bound_fns);

        // Filter to only new instances
        let new: HashMap<MonoKey, HashMap<String, Ty>> = instances.into_iter()
            .filter(|(k, _)| !all_instances.contains_key(k))
            .collect();
        if new.is_empty() {
            break;
        }

        // Clone and specialize new functions
        let mut new_functions = Vec::new();
        for ((fn_name, suffix), bindings) in &new {
            if let Some(orig) = program.functions.iter().find(|f| f.name == *fn_name) {
                let specialized = specialize_function(orig, suffix, bindings);
                new_functions.push(specialized);
            }
        }

        // Rewrite call sites (all instances, including previous rounds)
        all_instances.extend(new);
        rewrite_calls(program, &bound_fns, &all_instances);

        // Add specialized functions so next round can discover transitive calls in them
        program.functions.extend(new_functions);
    }

    // 元の generic/open-record 関数を削除（specialized 版が代わりに使われる）
    // instance が見つかった関数のみ削除
    let mono_fn_names: std::collections::HashSet<String> = all_instances.keys().map(|(name, _)| name.clone()).collect();
    program.functions.retain(|f| !mono_fn_names.contains(&f.name));
}

/// Info about a structurally-bounded type parameter in a function.
struct BoundedParam {
    /// Index of the parameter in the function signature
    param_idx: usize,
    /// Name of the type variable (e.g., "T")
    type_var: String,
}

/// Find functions that have structural bounds on generic type parameters,
/// OR direct OpenRecord parameters.
/// Returns function_name → list of bounded params.
fn find_structurally_bounded_fns(functions: &[IrFunction], type_decls: &[IrTypeDecl]) -> HashMap<String, Vec<BoundedParam>> {
    let mut result = HashMap::new();
    for func in functions {
        let mut bounded = Vec::new();
        // パターン A: generic + structural bound (fn f[T: { name: String, .. }](x: T))
        if let Some(ref generics) = func.generics {
            bounded.extend(
                generics.iter()
                    .filter(|g| g.structural_bound.is_some())
                    .flat_map(|g| {
                        func.params.iter().enumerate()
                            .filter(|(_, param)| ty_contains_typevar(&param.ty, &g.name))
                            .map(|(i, _)| BoundedParam { param_idx: i, type_var: g.name.clone() })
                    })
            );
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
        if !bounded.is_empty() {
            result.insert(func.name.clone(), bounded);
        }
    }
    result
}


/// Collect type variable bindings for a monomorphization call site.
fn collect_mono_bindings(
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
fn discover_instances(
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

fn discover_in_expr(
    expr: &IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    program_functions: &[IrFunction],
    instances: &mut HashMap<MonoKey, HashMap<String, Ty>>,
) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get(name) {
                    // Find the original function to get parameter types
                    let param_types: Vec<Ty> = program_functions.iter()
                        .find(|f| f.name == *name)
                        .map(|f| f.params.iter().map(|p| p.ty.clone()).collect())
                        .unwrap_or_default();

                    let bindings = collect_mono_bindings(bounded_params, args, &param_types);
                    // Skip bindings with Unknown or unresolved inference vars
                    let all_concrete = !bindings.is_empty() && bindings.values().all(|ty|
                        !matches!(ty, Ty::Unknown) && !ty.contains_unknown()
                        && !matches!(ty, Ty::TypeVar(n) if n.starts_with('?'))
                    );
                    if all_concrete {
                        let suffix = mangle_suffix(&bindings);
                        instances.insert((name.clone(), suffix), bindings);
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
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
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

fn discover_in_stmt(
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
        IrStmtKind::FieldAssign { value, .. } => discover_in_expr(value, bound_fns, program_functions, instances),
        IrStmtKind::Expr { expr } => discover_in_expr(expr, bound_fns, program_functions, instances),
        IrStmtKind::Guard { cond, else_ } => {
            discover_in_expr(cond, bound_fns, program_functions, instances);
            discover_in_expr(else_, bound_fns, program_functions, instances);
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
        Ty::Applied(crate::types::TypeConstructorId::List, args) if args.len() == 1 => format!("List_{}", mangle_ty(&args[0])),
        Ty::Applied(id, args) => {
            let name = format!("{:?}", id);
            if args.is_empty() { name } else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
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
        // OpenRecord パラメータ (直接 or エイリアス) → 具体型に直接置換
        let param_pos = orig.params.iter().position(|p| p.var == param.var).unwrap_or(0);
        let open_key = format!("__open_{}", param_pos);
        if bindings.contains_key(&open_key) {
            if let Some(concrete) = bindings.get(&open_key) {
                param.ty = concrete.clone();
            }
        } else {
            param.ty = substitute_ty(&param.ty, bindings);
        }
    }
    func.ret_ty = substitute_ty(&func.ret_ty, bindings);
    substitute_expr_types(&mut func.body, bindings);

    func
}

/// Substitute TypeVars with concrete types.
/// Uses Ty::map_children for uniform recursive traversal.
fn substitute_ty(ty: &Ty, bindings: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        // In IR, TypeVar("T") may appear as Named("T", [])
        Ty::Named(name, args) if args.is_empty() && bindings.contains_key(name) => {
            bindings[name].clone()
        }
        Ty::OpenRecord { .. } => {
            // OpenRecord パラメータを具体型に置換（__open_N → 具体型）
            for (_, concrete) in bindings.iter() {
                if let Ty::Named(_, _) | Ty::Record { .. } = concrete {
                    return concrete.clone();
                }
            }
            ty.map_children(&|child| substitute_ty(child, bindings))
        }
        // All other types: recursively substitute children
        _ => ty.map_children(&|child| substitute_ty(child, bindings)),
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
    // Pre-collect parameter types for bound functions (needed for type extraction)
    let fn_param_types: HashMap<String, Vec<Ty>> = program.functions.iter()
        .filter(|f| bound_fns.contains_key(&f.name))
        .map(|f| (f.name.clone(), f.params.iter().map(|p| p.ty.clone()).collect()))
        .collect();

    for func in &mut program.functions {
        rewrite_expr_calls(&mut func.body, bound_fns, instances, &fn_param_types);
    }
    for tl in &mut program.top_lets {
        rewrite_expr_calls(&mut tl.value, bound_fns, instances, &fn_param_types);
    }
}

fn rewrite_expr_calls(
    expr: &mut IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
    fn_param_types: &HashMap<String, Vec<Ty>>,
) {
    match &mut expr.kind {
        IrExprKind::Call { target, args, .. } => {
            for a in args.iter_mut() { rewrite_expr_calls(a, bound_fns, instances, fn_param_types); }
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get(name.as_str()) {
                    let param_types = fn_param_types.get(name.as_str());
                    let pt = param_types.map(|pts| pts.as_slice()).unwrap_or(&[]);
                    let bindings = collect_mono_bindings(bounded_params, args, pt);
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
                    rewrite_expr_calls(object, bound_fns, instances, fn_param_types);
                }
                _ => {}
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            rewrite_expr_calls(left, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(right, bound_fns, instances, fn_param_types);
        }
        IrExprKind::UnOp { operand, .. } => rewrite_expr_calls(operand, bound_fns, instances, fn_param_types),
        IrExprKind::If { cond, then, else_ } => {
            rewrite_expr_calls(cond, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(then, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(else_, bound_fns, instances, fn_param_types);
        }
        IrExprKind::Match { subject, arms } => {
            rewrite_expr_calls(subject, bound_fns, instances, fn_param_types);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_expr_calls(g, bound_fns, instances, fn_param_types); }
                rewrite_expr_calls(&mut arm.body, bound_fns, instances, fn_param_types);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types); }
            if let Some(e) = expr { rewrite_expr_calls(e, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            rewrite_expr_calls(iterable, bound_fns, instances, fn_param_types);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::While { cond, body } => {
            rewrite_expr_calls(cond, bound_fns, instances, fn_param_types);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { rewrite_expr_calls(e, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            rewrite_expr_calls(base, bound_fns, instances, fn_param_types);
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances, fn_param_types); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                rewrite_expr_calls(k, bound_fns, instances, fn_param_types);
                rewrite_expr_calls(v, bound_fns, instances, fn_param_types);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            rewrite_expr_calls(start, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(end, bound_fns, instances, fn_param_types);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types);
        }
        IrExprKind::IndexAccess { object, index } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(index, bound_fns, instances, fn_param_types);
        }
        IrExprKind::Lambda { body, .. } => rewrite_expr_calls(body, bound_fns, instances, fn_param_types),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    rewrite_expr_calls(expr, bound_fns, instances, fn_param_types);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => rewrite_expr_calls(expr, bound_fns, instances, fn_param_types),
        _ => {}
    }
}

fn rewrite_stmt_calls(
    stmt: &mut IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
    fn_param_types: &HashMap<String, Vec<Ty>>,
) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => rewrite_expr_calls(value, bound_fns, instances, fn_param_types),
        IrStmtKind::IndexAssign { index, value, .. } => {
            rewrite_expr_calls(index, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(value, bound_fns, instances, fn_param_types);
        }
        IrStmtKind::FieldAssign { value, .. } => rewrite_expr_calls(value, bound_fns, instances, fn_param_types),
        IrStmtKind::Expr { expr } => rewrite_expr_calls(expr, bound_fns, instances, fn_param_types),
        IrStmtKind::Guard { cond, else_ } => {
            rewrite_expr_calls(cond, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(else_, bound_fns, instances, fn_param_types);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Check if a type contains a specific TypeVar anywhere in its structure.
/// Uses Ty::any_child_recursive for uniform traversal.
fn ty_contains_typevar(ty: &Ty, name: &str) -> bool {
    ty.any_child_recursive(&|t| match t {
        Ty::TypeVar(n) => n == name,
        Ty::Named(n, args) => n == name && args.is_empty(),
        _ => false,
    })
}

/// Extract the concrete type for a TypeVar by matching parameter type structure against argument type.
/// Uses Ty::constructor_id() and type_args() for uniform container matching.
fn extract_typevar_binding(param_ty: &Ty, arg_ty: &Ty, var_name: &str) -> Ty {
    match (param_ty, arg_ty) {
        (Ty::TypeVar(n), _) if n == var_name => arg_ty.clone(),
        (Ty::Named(n, _), _) if n == var_name => arg_ty.clone(),
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
            arg_ty.clone() // fallback
        }
    }
}
