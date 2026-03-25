use crate::ir::*;
use crate::types::Ty;
use super::utils::has_typevar;

pub(super) fn propagate_concrete_types(program: &mut IrProgram) {
    for func in &mut program.functions {
        propagate_expr(&mut func.body, &mut program.var_table);
        // If function body is a match (or block ending in match) and its type is wrong,
        // override with function's ret_ty (which mono has correctly substituted)
        fix_body_match_ty(&mut func.body, &func.ret_ty);
    }
    for tl in &mut program.top_lets {
        propagate_expr(&mut tl.value, &mut program.var_table);
    }
}

/// If the body expression is a Match whose .ty disagrees with ret_ty, fix it.
/// Also recurse into Block tails.
fn fix_body_match_ty(body: &mut IrExpr, ret_ty: &Ty) {
    match &mut body.kind {
        IrExprKind::Match { arms, .. } => {
            if crate::codegen::emit_wasm::values::ty_to_valtype(&body.ty) != crate::codegen::emit_wasm::values::ty_to_valtype(ret_ty)
                && !matches!(ret_ty, Ty::Unit | Ty::Unknown)
            {
                body.ty = ret_ty.clone();
                // Also fix arm body types
                for arm in arms.iter_mut() {
                    if crate::codegen::emit_wasm::values::ty_to_valtype(&arm.body.ty) != crate::codegen::emit_wasm::values::ty_to_valtype(ret_ty) {
                        fix_body_match_ty(&mut arm.body, ret_ty);
                    }
                }
            }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            fix_body_match_ty(tail, ret_ty);
            if crate::codegen::emit_wasm::values::ty_to_valtype(&body.ty) != crate::codegen::emit_wasm::values::ty_to_valtype(ret_ty)
                && !matches!(ret_ty, Ty::Unit | Ty::Unknown) {
                body.ty = ret_ty.clone();
            }
        }
        _ => {}
    }
}

fn propagate_expr(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
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
                propagate_pattern_types_mut(&mut arm.pattern, &subj_ty, vt);
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

fn propagate_pattern_types_mut(pattern: &mut IrPattern, subject_ty: &Ty, vt: &mut VarTable) {
    match pattern {
        IrPattern::Bind { var, ty } => {
            // Update pattern.ty from VarTable (which mono/propagate has made concrete)
            let vt_ty = &vt.get(*var).ty;
            if has_typevar(ty) && !has_typevar(vt_ty) {
                *ty = vt_ty.clone();
            }
        }
        IrPattern::Constructor { args, .. } => {
            for a in args { propagate_pattern_types_mut(a, subject_ty, vt); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { propagate_pattern_types_mut(e, subject_ty, vt); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            propagate_pattern_types_mut(inner, subject_ty, vt);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &mut f.pattern { propagate_pattern_types_mut(p, subject_ty, vt); } }
        }
        _ => {}
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
