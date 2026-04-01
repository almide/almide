use std::collections::HashMap;
use crate::ir::*;
use crate::types::Ty;
use super::utils::ty_to_name;

/// Specialize a function for concrete types.
/// Builds the specialized function directly without cloning the entire original,
/// avoiding redundant allocation of fields we immediately overwrite (generics, name).
pub(super) fn specialize_function(
    orig: &IrFunction,
    suffix: &str,
    bindings: &HashMap<String, Ty>,
) -> IrFunction {
    // Build specialized params: substitute type variables
    let params: Vec<IrParam> = orig.params.iter().enumerate().map(|(i, param)| {
        let open_key = format!("__open_{}", i);
        let new_ty = if let Some(concrete) = bindings.get(&open_key) {
            concrete.clone()
        } else {
            substitute_ty(&param.ty, bindings)
        };
        IrParam { ty: new_ty, ..param.clone() }
    }).collect();

    // Build specialized body: clone + substitute in one step
    let mut body = orig.body.clone();
    substitute_expr_types(&mut body, bindings);

    IrFunction {
        name: format!("{}__{}", orig.name, suffix).into(),
        params,
        ret_ty: substitute_ty(&orig.ret_ty, bindings),
        body,
        generics: None, // specialized function is concrete
        is_effect: orig.is_effect,
        is_async: orig.is_async,
        is_test: orig.is_test,
        extern_attrs: orig.extern_attrs.clone(),
        visibility: orig.visibility.clone(),
        doc: None,
        blank_lines_before: 0,
    }
}

/// Substitute TypeVars with concrete types.
/// Uses Ty::map_children for uniform recursive traversal.
pub(super) fn substitute_ty(ty: &Ty, bindings: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => bindings.get(name.as_str()).cloned().unwrap_or_else(|| ty.clone()),
        // In IR, TypeVar("T") may appear as Named("T", [])
        Ty::Named(name, args) if args.is_empty() && bindings.contains_key(name.as_str()) => {
            bindings[name.as_str()].clone()
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
pub(super) fn update_var_table_types(expr: &IrExpr, bindings: &HashMap<String, Ty>, vt: &mut VarTable) {
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
        IrExprKind::Block { stmts, expr } => {
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
        IrExprKind::Lambda { body, params, .. } => {
            for (var_id, _) in params {
                let new_ty = substitute_ty(&vt.get(*var_id).ty, bindings);
                vt.entries[var_id.0 as usize].ty = new_ty;
            }
            update_var_table_types(body, bindings, vt);
        }
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
        IrPattern::Bind { var, .. } => { vt.entries[var.0 as usize].ty = substitute_ty(&vt.get(*var).ty, bindings); }
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
        IrStmtKind::ListSwap { a, b, .. } => { update_var_table_types(a, bindings, vt); update_var_table_types(b, bindings, vt); }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => { update_var_table_types(end, bindings, vt); }
        IrStmtKind::ListCopySlice { len, .. } => { update_var_table_types(len, bindings, vt); }
        IrStmtKind::Expr { expr } => update_var_table_types(expr, bindings, vt),
        IrStmtKind::Guard { cond, else_ } => { update_var_table_types(cond, bindings, vt); update_var_table_types(else_, bindings, vt); }
        IrStmtKind::Comment { .. } => {}
    }
}

pub(super) fn substitute_expr_types(expr: &mut IrExpr, bindings: &HashMap<String, Ty>) {
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
                substitute_pattern_types(&mut arm.pattern, bindings);
                if let Some(g) = &mut arm.guard { substitute_expr_types(g, bindings); }
                substitute_expr_types(&mut arm.body, bindings);
            }
        }
        IrExprKind::Block { stmts, expr } => {
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
                                *method = format!("{}.{}", concrete_name, method_name).into();
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

fn substitute_pattern_types(pattern: &mut IrPattern, bindings: &HashMap<String, Ty>) {
    match pattern {
        IrPattern::Bind { ty, .. } => { *ty = substitute_ty(ty, bindings); }
        IrPattern::Constructor { args, .. } => { for a in args { substitute_pattern_types(a, bindings); } }
        IrPattern::Tuple { elements } => { for e in elements { substitute_pattern_types(e, bindings); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => { substitute_pattern_types(inner, bindings); }
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &mut f.pattern { substitute_pattern_types(p, bindings); } } }
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
        IrStmtKind::ListSwap { a, b, .. } => {
            substitute_expr_types(a, bindings);
            substitute_expr_types(b, bindings);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            substitute_expr_types(end, bindings);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            substitute_expr_types(len, bindings);
        }
        IrStmtKind::Expr { expr } => substitute_expr_types(expr, bindings),
        IrStmtKind::Guard { cond, else_ } => {
            substitute_expr_types(cond, bindings);
            substitute_expr_types(else_, bindings);
        }
        IrStmtKind::Comment { .. } => {}
    }
}
