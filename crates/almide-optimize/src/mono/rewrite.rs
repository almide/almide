use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::Ty;
use super::utils::{MonoKey, BoundedParam, mangle_suffix};
use super::discovery::{collect_mono_bindings, extract_typevar_binding};
use super::specialization::substitute_ty;

/// Rewrite call sites to point to specialized functions.
pub(super) fn rewrite_calls(
    program: &mut IrProgram,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
) {
    // Tests can share raw names with generic functions (e.g. `fn wrap_all[T]` vs
    // `test "wrap_all"` — both lower to name = "wrap_all"). Skip tests when
    // building these lookups so the generic function's signature wins.
    let fn_param_types: HashMap<String, Vec<Ty>> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .map(|f| (f.name.to_string(), f.params.iter().map(|p| p.ty.clone()).collect()))
        .collect();
    let fn_generics: HashMap<String, Vec<String>> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .filter_map(|f| f.generics.as_ref().map(|gs| (f.name.to_string(), gs.iter().map(|g| g.name.to_string()).collect())))
        .collect();
    let fn_ret_types: HashMap<String, Ty> = program.functions.iter()
        .filter(|f| !f.is_test && bound_fns.contains_key::<str>(&f.name))
        .map(|f| (f.name.to_string(), f.ret_ty.clone()))
        .collect();

    for func in &mut program.functions {
        rewrite_expr_calls(&mut func.body, bound_fns, instances, &fn_param_types, &fn_generics, &fn_ret_types);
    }
    for tl in &mut program.top_lets {
        rewrite_expr_calls(&mut tl.value, bound_fns, instances, &fn_param_types, &fn_generics, &fn_ret_types);
    }
}

fn rewrite_expr_calls(
    expr: &mut IrExpr,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
    fn_param_types: &HashMap<String, Vec<Ty>>,
    fn_generics: &HashMap<String, Vec<String>>,
    fn_ret_types: &HashMap<String, Ty>,
) {
    match &mut expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            for a in args.iter_mut() { rewrite_expr_calls(a, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
            if let CallTarget::Named { name } = target {
                if let Some(bounded_params) = bound_fns.get(name.as_str()) {
                    let param_types = fn_param_types.get(name.as_str());
                    let pt = param_types.map(|pts| pts.as_slice()).unwrap_or(&[]);
                    let mut bindings = collect_mono_bindings(bounded_params, args, pt);

                    // Supplement from explicit type_args
                    if !type_args.is_empty() {
                        if let Some(gnames) = fn_generics.get(name.as_str()) {
                            for (gname, ta) in gnames.iter().zip(type_args.iter()) {
                                if !bindings.contains_key(gname) || matches!(bindings.get(gname), Some(Ty::Unknown)) {
                                    bindings.insert(gname.clone(), ta.clone());
                                }
                            }
                        }
                    }

                    // Infer from call expr.ty vs function ret_ty (for paramless generics)
                    if bindings.is_empty() || bindings.values().any(|v| matches!(v, Ty::Unknown)) {
                        if let Some(gnames) = fn_generics.get(name.as_str()) {
                            if let Some(ret_ty) = fn_ret_types.get(name.as_str()) {
                                for gname in gnames {
                                    if !bindings.contains_key(gname) || matches!(bindings.get(gname), Some(Ty::Unknown)) {
                                        let extracted = extract_typevar_binding(ret_ty, &expr.ty, gname);
                                        if !matches!(extracted, Ty::Unknown) {
                                            bindings.insert(gname.clone(), extracted);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !bindings.is_empty() {
                        let suffix = mangle_suffix(&bindings);
                        if instances.contains_key(&(name.to_string(), suffix.clone())) {
                            *name = format!("{}__{}", name, suffix).into();
                            expr.ty = substitute_ty(&expr.ty, &bindings);
                        }
                    }
                }
            }
            match target {
                CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => {
                    rewrite_expr_calls(object, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
                }
                _ => {}
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            rewrite_expr_calls(left, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(right, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::UnOp { operand, .. } => rewrite_expr_calls(operand, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types),
        IrExprKind::If { cond, then, else_ } => {
            rewrite_expr_calls(cond, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(then, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(else_, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::Match { subject, arms } => {
            rewrite_expr_calls(subject, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_expr_calls(g, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
                rewrite_expr_calls(&mut arm.body, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
            if let Some(e) = expr { rewrite_expr_calls(e, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            rewrite_expr_calls(iterable, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::While { cond, body } => {
            rewrite_expr_calls(cond, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            for s in body { rewrite_stmt_calls(s, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { rewrite_expr_calls(e, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            rewrite_expr_calls(base, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            for (_, e) in fields { rewrite_expr_calls(e, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                rewrite_expr_calls(k, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
                rewrite_expr_calls(v, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            rewrite_expr_calls(start, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(end, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::IndexAccess { object, index } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(index, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::MapAccess { object, key } => {
            rewrite_expr_calls(object, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
            rewrite_expr_calls(key, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
        }
        IrExprKind::Lambda { body, .. } => rewrite_expr_calls(body, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    rewrite_expr_calls(expr, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => rewrite_expr_calls(expr, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types),
        _ => {}
    }
}

fn rewrite_stmt_calls(
    stmt: &mut IrStmt,
    bound_fns: &HashMap<String, Vec<BoundedParam>>,
    instances: &HashMap<MonoKey, HashMap<String, Ty>>,
    fn_param_types: &HashMap<String, Vec<Ty>>,
    fn_generics: &HashMap<String, Vec<String>>,
    fn_ret_types: &HashMap<String, Ty>,
) {
    let rw = |e: &mut IrExpr| rewrite_expr_calls(e, bound_fns, instances, fn_param_types, fn_generics, fn_ret_types);
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => rw(value),
        IrStmtKind::IndexAssign { index, value, .. } => { rw(index); rw(value); }
        IrStmtKind::MapInsert { key, value, .. } => { rw(key); rw(value); }
        IrStmtKind::FieldAssign { value, .. } => rw(value),
        IrStmtKind::ListSwap { a, b, .. } => { rw(a); rw(b); }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => { rw(end); }
        IrStmtKind::ListCopySlice { len, .. } => { rw(len); }
        IrStmtKind::Expr { expr } => rw(expr),
        IrStmtKind::Guard { cond, else_ } => { rw(cond); rw(else_); }
        IrStmtKind::Comment { .. } => {}
    }
}
