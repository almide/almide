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
/// Specialize generic functions for concrete type arguments at each call site.
pub fn monomorphize(program: &mut IrProgram) {
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

        // Add specialized functions so next round can discover transitive calls in them
        program.functions.extend(new_functions);
    }

    // 元の generic/open-record 関数を削除（specialized 版が代わりに使われる）
    let mono_fn_names: std::collections::HashSet<String> = all_instances.keys().map(|(name, _)| name.clone()).collect();
    program.functions.retain(|f| !mono_fn_names.contains(&f.name));

    // Propagate concrete types: after rewrite, some expressions still have TypeVar
    // types (e.g., `let x = mono_fn(...)` where x.ty was set before mono).
    // Walk every function and fix: Bind.ty ← value.ty, Var.ty ← VarTable, Match.ty ← arm bodies.
    propagate_concrete_types(program);
}

/// Info about a structurally-bounded type parameter in a function.
struct BoundedParam {
    /// Index of the parameter in the function signature
    param_idx: usize,
    /// Name of the type variable (e.g., "T")
    type_var: String,
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

/// Generate a mangled suffix from type variable bindings.
fn mangle_suffix(bindings: &HashMap<String, Ty>) -> String {
    let mut entries: Vec<(&String, &Ty)> = bindings.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    entries.iter().map(|(_, ty)| mangle_ty(ty)).collect::<Vec<_>>().join("_")
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

/// Update VarTable types for all variables referenced in an expression tree.
fn update_var_table_types(expr: &IrExpr, bindings: &HashMap<String, Ty>, vt: &mut VarTable) {
    if let IrExprKind::Var { id } = &expr.kind {
        let old = &vt.get(*id).ty;
        let new = substitute_ty(old, bindings);
        if new != *old {
            vt.entries[id.0 as usize].ty = new;
        }
    }
    // Recurse using the same structure as substitute_expr_types
    match &expr.kind {
        IrExprKind::BinOp { left, right, .. } => { update_var_table_types(left, bindings, vt); update_var_table_types(right, bindings, vt); }
        IrExprKind::UnOp { operand, .. } => update_var_table_types(operand, bindings, vt),
        IrExprKind::If { cond, then, else_ } => { update_var_table_types(cond, bindings, vt); update_var_table_types(then, bindings, vt); update_var_table_types(else_, bindings, vt); }
        IrExprKind::Match { subject, arms } => {
            update_var_table_types(subject, bindings, vt);
            for arm in arms {
                update_pattern_var_types(&arm.pattern, bindings, vt);
                if let Some(g) = &arm.guard { update_var_table_types(g, bindings, vt); }
                update_var_table_types(&arm.body, bindings, vt);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { update_stmt_var_types(s, bindings, vt); }
            if let Some(e) = expr { update_var_table_types(e, bindings, vt); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => update_var_table_types(object, bindings, vt),
                _ => {}
            }
            for a in args { update_var_table_types(a, bindings, vt); }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            // Update loop variable types
            let new = substitute_ty(&vt.get(*var).ty, bindings);
            vt.entries[var.0 as usize].ty = new;
            if let Some(tvs) = var_tuple { for tv in tvs { vt.entries[tv.0 as usize].ty = substitute_ty(&vt.get(*tv).ty, bindings); } }
            update_var_table_types(iterable, bindings, vt);
            for s in body { update_stmt_var_types(s, bindings, vt); }
        }
        IrExprKind::While { cond, body } => {
            update_var_table_types(cond, bindings, vt);
            for s in body { update_stmt_var_types(s, bindings, vt); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => { for e in elements { update_var_table_types(e, bindings, vt); } }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => { for (_, e) in fields { update_var_table_types(e, bindings, vt); } }
        IrExprKind::Lambda { body, .. } => update_var_table_types(body, bindings, vt),
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::Try { expr } | IrExprKind::Await { expr } | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => {
            update_var_table_types(expr, bindings, vt);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } | IrExprKind::IndexAccess { object, .. } => {
            update_var_table_types(object, bindings, vt);
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { update_var_table_types(k, bindings, vt); update_var_table_types(v, bindings, vt); } }
        IrExprKind::StringInterp { parts } => { for p in parts { if let IrStringPart::Expr { expr } = p { update_var_table_types(expr, bindings, vt); } } }
        _ => {}
    }
}

fn update_pattern_var_types(pattern: &IrPattern, bindings: &HashMap<String, Ty>, vt: &mut VarTable) {
    match pattern {
        IrPattern::Bind { var } => { vt.entries[var.0 as usize].ty = substitute_ty(&vt.get(*var).ty, bindings); }
        IrPattern::Constructor { args, .. } => { for a in args { update_pattern_var_types(a, bindings, vt); } }
        IrPattern::Tuple { elements } => { for e in elements { update_pattern_var_types(e, bindings, vt); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => { update_pattern_var_types(inner, bindings, vt); }
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &f.pattern { update_pattern_var_types(p, bindings, vt); } } }
        _ => {}
    }
}

fn update_stmt_var_types(stmt: &IrStmt, bindings: &HashMap<String, Ty>, vt: &mut VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => {
            vt.entries[var.0 as usize].ty = substitute_ty(&vt.get(*var).ty, bindings);
            update_var_table_types(value, bindings, vt);
        }
        IrStmtKind::BindDestructure { value, pattern, .. } => {
            update_pattern_var_types(pattern, bindings, vt);
            update_var_table_types(value, bindings, vt);
        }
        IrStmtKind::Assign { value, .. } => update_var_table_types(value, bindings, vt),
        IrStmtKind::IndexAssign { index, value, .. } => { update_var_table_types(index, bindings, vt); update_var_table_types(value, bindings, vt); }
        IrStmtKind::MapInsert { key, value, .. } => { update_var_table_types(key, bindings, vt); update_var_table_types(value, bindings, vt); }
        IrStmtKind::FieldAssign { value, .. } => update_var_table_types(value, bindings, vt),
        IrStmtKind::Expr { expr } => update_var_table_types(expr, bindings, vt),
        IrStmtKind::Guard { cond, else_ } => { update_var_table_types(cond, bindings, vt); update_var_table_types(else_, bindings, vt); }
        IrStmtKind::Comment { .. } => {}
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
                CallTarget::Method { object, method } => {
                    substitute_expr_types(object, bindings);
                    // Rewrite protocol method calls: T.show → Dog.show when T → Dog
                    if let Some(dot_pos) = method.find('.') {
                        let tv_name = &method[..dot_pos];
                        if let Some(concrete_ty) = bindings.get(tv_name) {
                            if let Some(concrete_name) = ty_to_name(concrete_ty) {
                                let method_name = &method[dot_pos+1..];
                                *method = format!("{}.{}", concrete_name, method_name);
                            }
                        }
                    }
                }
                CallTarget::Computed { callee: object } => {
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
        IrExprKind::MapAccess { object, key } => {
            substitute_expr_types(object, bindings);
            substitute_expr_types(key, bindings);
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
        IrStmtKind::MapInsert { key, value, .. } => {
            substitute_expr_types(key, bindings);
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
                            // Also update the call expression's type with concrete bindings
                            expr.ty = substitute_ty(&expr.ty, &bindings);
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
        IrExprKind::MapAccess { object, key } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types);
            rewrite_expr_calls(key, bound_fns, instances, fn_param_types);
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
        IrStmtKind::MapInsert { key, value, .. } => {
            rewrite_expr_calls(key, bound_fns, instances, fn_param_types);
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

/// Extract the concrete type name from a Ty for protocol method rewriting.
fn ty_to_name(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Named(name, _) => Some(name.clone()),
        Ty::Int => Some("Int".into()),
        Ty::Float => Some("Float".into()),
        Ty::String => Some("String".into()),
        Ty::Bool => Some("Bool".into()),
        _ => None,
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

// ── Post-mono type propagation ──────────────────────────────────────
//
// After monomorphization rewrites call names, some expressions in caller
// functions still carry generic types (e.g., `let x = maybe_map(...)` where
// x.ty = Maybe[TypeVar("B")]). This pass walks the entire IR bottom-up and
// propagates concrete types from values to bindings and from VarTable to Var
// expressions, eliminating residual TypeVars.

fn propagate_concrete_types(program: &mut IrProgram) {
    for func in &mut program.functions {
        propagate_expr(&mut func.body, &mut program.var_table);
    }
    for tl in &mut program.top_lets {
        propagate_expr(&mut tl.value, &mut program.var_table);
    }
}

fn has_typevar(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| {
        matches!(t, Ty::TypeVar(_))
            || matches!(t, Ty::Named(n, args) if args.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
    })
}

fn propagate_expr(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts.iter_mut() { propagate_stmt(s, vt); }
            if let Some(e) = tail { propagate_expr(e, vt); }
            // Block type = tail type
            if let Some(e) = tail {
                if has_typevar(&expr.ty) && !has_typevar(&e.ty) {
                    expr.ty = e.ty.clone();
                }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            propagate_expr(cond, vt);
            propagate_expr(then, vt);
            propagate_expr(else_, vt);
            if has_typevar(&expr.ty) && !has_typevar(&then.ty) { expr.ty = then.ty.clone(); }
        }
        IrExprKind::Match { subject, arms } => {
            propagate_expr(subject, vt);
            // Propagate concrete types into pattern bindings
            let subj_ty = subject.ty.clone();
            for arm in arms.iter_mut() {
                propagate_pattern_types(&arm.pattern, &subj_ty, vt);
                if let Some(g) = &mut arm.guard { propagate_expr(g, vt); }
                propagate_expr(&mut arm.body, vt);
            }
            // Match type = first concrete arm body type
            if has_typevar(&expr.ty) {
                for arm in arms.iter() {
                    if !has_typevar(&arm.body.ty) {
                        expr.ty = arm.body.ty.clone();
                        break;
                    }
                }
            }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => propagate_expr(object, vt),
                _ => {}
            }
            for a in args.iter_mut() { propagate_expr(a, vt); }
        }
        IrExprKind::Var { id } => {
            // Sync Var type with VarTable
            let vt_ty = &vt.get(*id).ty;
            if has_typevar(&expr.ty) && !has_typevar(vt_ty) {
                expr.ty = vt_ty.clone();
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            propagate_expr(iterable, vt);
            for s in body.iter_mut() { propagate_stmt(s, vt); }
        }
        IrExprKind::While { cond, body } => {
            propagate_expr(cond, vt);
            for s in body.iter_mut() { propagate_stmt(s, vt); }
        }
        IrExprKind::BinOp { left, right, .. } => { propagate_expr(left, vt); propagate_expr(right, vt); }
        IrExprKind::UnOp { operand, .. } => propagate_expr(operand, vt),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements.iter_mut() { propagate_expr(e, vt); }
        }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, e) in fields.iter_mut() { propagate_expr(e, vt); }
        }
        IrExprKind::Lambda { body, .. } => propagate_expr(body, vt),
        IrExprKind::OptionSome { expr: inner } | IrExprKind::ResultOk { expr: inner }
        | IrExprKind::ResultErr { expr: inner } | IrExprKind::Try { expr: inner }
        | IrExprKind::Await { expr: inner } | IrExprKind::Clone { expr: inner }
        | IrExprKind::Deref { expr: inner } => propagate_expr(inner, vt),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::IndexAccess { object, .. } => propagate_expr(object, vt),
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries.iter_mut() { propagate_expr(k, vt); propagate_expr(v, vt); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts.iter_mut() {
                if let IrStringPart::Expr { expr: e } = p { propagate_expr(e, vt); }
            }
        }
        IrExprKind::Range { start, end, .. } => { propagate_expr(start, vt); propagate_expr(end, vt); }
        IrExprKind::MapAccess { object, key } => { propagate_expr(object, vt); propagate_expr(key, vt); }
        _ => {}
    }
}

/// Propagate concrete types from match subject into pattern-bound variables.
/// When subject_ty is concrete (e.g., Maybe[Int]), update VarTable for Bind vars
/// inside Constructor/Some/Ok/Err patterns so they get the concrete payload type.
fn propagate_pattern_types(pattern: &IrPattern, subject_ty: &Ty, vt: &mut VarTable) {
    match pattern {
        IrPattern::Constructor { args, .. } => {
            // Constructor payload: for each bound arg, if VarTable has TypeVar,
            // try to resolve from the subject_ty's variant fields
            for arg in args {
                if let IrPattern::Bind { var } = arg {
                    let cur = &vt.get(*var).ty;
                    if has_typevar(cur) {
                        // Look up the variant's field types from type registry
                        // For now: if subject is Named with type_args, substitute
                        // the var's type using those args as generic bindings
                        if let Ty::Named(_, type_args) = subject_ty {
                            if !type_args.is_empty() {
                                let old = cur.clone();
                                let mut generic_names: Vec<&str> = Vec::new();
                                collect_type_param_names_from_ty(&old, &mut generic_names);
                                let new = substitute_named_type_params(&old, &generic_names, type_args);
                                if new != old {
                                    vt.entries[var.0 as usize].ty = new;
                                }
                            }
                        }
                    }
                }
            }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } => {
            if let IrPattern::Bind { var } = inner.as_ref() {
                let inner_ty = match subject_ty {
                    Ty::Applied(_, args) if !args.is_empty() => Some(args[0].clone()),
                    _ => None,
                };
                if let Some(ty) = inner_ty {
                    if has_typevar(&vt.get(*var).ty) && !has_typevar(&ty) {
                        vt.entries[var.0 as usize].ty = ty;
                    }
                }
            }
        }
        IrPattern::Err { inner } => {
            if let IrPattern::Bind { var } = inner.as_ref() {
                let inner_ty = match subject_ty {
                    Ty::Applied(_, args) if args.len() >= 2 => Some(args[1].clone()),
                    _ => None,
                };
                if let Some(ty) = inner_ty {
                    if has_typevar(&vt.get(*var).ty) && !has_typevar(&ty) {
                        vt.entries[var.0 as usize].ty = ty;
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_type_param_names_from_ty<'a>(ty: &'a Ty, names: &mut Vec<&'a str>) {
    match ty {
        Ty::Named(n, args) if args.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()) => {
            if !names.contains(&n.as_str()) { names.push(n.as_str()); }
        }
        Ty::TypeVar(n) => { if !names.contains(&n.as_str()) { names.push(n.as_str()); } }
        Ty::Applied(_, args) | Ty::Named(_, args) => { for a in args { collect_type_param_names_from_ty(a, names); } }
        Ty::Tuple(elems) => { for e in elems { collect_type_param_names_from_ty(e, names); } }
        Ty::Fn { params, ret } => { for p in params { collect_type_param_names_from_ty(p, names); } collect_type_param_names_from_ty(ret, names); }
        _ => {}
    }
}

fn substitute_named_type_params(ty: &Ty, generic_names: &[&str], type_args: &[Ty]) -> Ty {
    match ty {
        Ty::Named(n, args) if args.is_empty() => {
            if let Some(idx) = generic_names.iter().position(|&g| g == n.as_str()) {
                if let Some(concrete) = type_args.get(idx) { return concrete.clone(); }
            }
            ty.clone()
        }
        Ty::TypeVar(n) => {
            if let Some(idx) = generic_names.iter().position(|&g| g == n.as_str()) {
                if let Some(concrete) = type_args.get(idx) { return concrete.clone(); }
            }
            ty.clone()
        }
        _ => ty.map_children(&|child| substitute_named_type_params(child, generic_names, type_args)),
    }
}

fn propagate_stmt(stmt: &mut IrStmt, vt: &mut VarTable) {
    match &mut stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            propagate_expr(value, vt);
            // Sync Bind type and VarTable with value's concrete type
            if has_typevar(ty) && !has_typevar(&value.ty) {
                *ty = value.ty.clone();
                vt.entries[var.0 as usize].ty = value.ty.clone();
            }
        }
        IrStmtKind::BindDestructure { value, .. } => propagate_expr(value, vt),
        IrStmtKind::Assign { value, .. } => propagate_expr(value, vt),
        IrStmtKind::IndexAssign { index, value, .. } => { propagate_expr(index, vt); propagate_expr(value, vt); }
        IrStmtKind::MapInsert { key, value, .. } => { propagate_expr(key, vt); propagate_expr(value, vt); }
        IrStmtKind::FieldAssign { value, .. } => propagate_expr(value, vt),
        IrStmtKind::Expr { expr } => propagate_expr(expr, vt),
        IrStmtKind::Guard { cond, else_ } => { propagate_expr(cond, vt); propagate_expr(else_, vt); }
        IrStmtKind::Comment { .. } => {}
    }
}
