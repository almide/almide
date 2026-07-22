use almide_ir::*;
use almide_lang::types::Ty;
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
    if matches!(ret_ty, Ty::Unit | Ty::Unknown) { return; }
    match &mut body.kind {
        IrExprKind::Match { arms, .. } => {
            if !almide_ir::wasm_types_compatible(&body.ty, ret_ty) {
                body.ty = ret_ty.clone();
                for arm in arms.iter_mut() {
                    if !almide_ir::wasm_types_compatible(&arm.body.ty, ret_ty) {
                        fix_body_match_ty(&mut arm.body, ret_ty);
                    }
                }
            }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            fix_body_match_ty(tail, ret_ty);
            if !almide_ir::wasm_types_compatible(&body.ty, ret_ty) {
                body.ty = ret_ty.clone();
            }
        }
        IrExprKind::If { then, else_, .. } => {
            fix_body_match_ty(then, ret_ty);
            fix_body_match_ty(else_, ret_ty);
            if !almide_ir::wasm_types_compatible(&body.ty, ret_ty) {
                body.ty = ret_ty.clone();
            }
        }
        _ => {
            // Leaf expression with wrong type — fix directly
            if !almide_ir::wasm_types_compatible(&body.ty, ret_ty) {
                body.ty = ret_ty.clone();
            }
        }
    }
}

fn propagate_expr(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { .. } => propagate_expr_block(expr, vt),
        IrExprKind::If { .. } | IrExprKind::ForIn { .. } | IrExprKind::While { .. } => propagate_expr_control(expr, vt),
        IrExprKind::Match { .. } => propagate_expr_match(expr, vt),
        IrExprKind::Call { .. } => propagate_expr_call(expr, vt),
        IrExprKind::Var { id } => {
            // Sync Var type with VarTable
            let vt_ty = &vt.get(*id).ty;
            if has_typevar(&expr.ty) && !has_typevar(vt_ty) {
                expr.ty = vt_ty.clone();
            }
        }
        IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } => propagate_expr_binop(expr, vt),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::Range { .. } | IrExprKind::MapAccess { .. } => propagate_expr_containers(expr, vt),
        IrExprKind::Lambda { .. } => propagate_expr_lambda(expr, vt),
        IrExprKind::OptionSome { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::Try { .. }
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } => propagate_expr_wrap(expr, vt),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::IndexAccess { object, .. } => propagate_expr(object, vt),
        _ => {}
    }
}

/// Block: propagate into statements and tail, then sync the block's type with the tail's type.
fn propagate_expr_block(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    for s in stmts.iter_mut() { propagate_stmt(s, vt); }
    if let Some(e) = tail { propagate_expr(e, vt); }
    // Block type = tail type
    if let Some(e) = tail {
        if has_typevar(&expr.ty) && !has_typevar(&e.ty) {
            expr.ty = e.ty.clone();
        }
    }
}

/// If / ForIn / While: propagate into condition/iterable and bodies.
fn propagate_expr_control(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::If { cond, then, else_ } => {
            propagate_expr(cond, vt);
            propagate_expr(then, vt);
            propagate_expr(else_, vt);
            if has_typevar(&expr.ty) && !has_typevar(&then.ty) { expr.ty = then.ty.clone(); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            propagate_expr(iterable, vt);
            for s in body.iter_mut() { propagate_stmt(s, vt); }
        }
        IrExprKind::While { cond, body } => {
            propagate_expr(cond, vt);
            for s in body.iter_mut() { propagate_stmt(s, vt); }
        }
        _ => unreachable!(),
    }
}

/// Call: propagate into the receiver (if any) and arguments.
fn propagate_expr_call(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    match target {
        CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => propagate_expr(object, vt),
        _ => {}
    }
    for a in args.iter_mut() { propagate_expr(a, vt); }
}

/// BinOp / UnOp: propagate into operands.
fn propagate_expr_binop(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => { propagate_expr(left, vt); propagate_expr(right, vt); }
        IrExprKind::UnOp { operand, .. } => propagate_expr(operand, vt),
        _ => unreachable!(),
    }
}

/// List/Tuple/Record/SpreadRecord/MapLiteral/StringInterp/Range/MapAccess: propagate into each child.
fn propagate_expr_containers(expr: &mut IrExpr, vt: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements.iter_mut() { propagate_expr(e, vt); }
        }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, e) in fields.iter_mut() { propagate_expr(e, vt); }
        }
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
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await/Clone/Deref: propagate into the wrapped expression.
fn propagate_expr_wrap(expr: &mut IrExpr, vt: &mut VarTable) {
    let (IrExprKind::OptionSome { expr: inner } | IrExprKind::ResultOk { expr: inner }
        | IrExprKind::ResultErr { expr: inner } | IrExprKind::Try { expr: inner }
        | IrExprKind::Await { expr: inner } | IrExprKind::Clone { expr: inner }
        | IrExprKind::Deref { expr: inner }) = &mut expr.kind else { unreachable!() };
    propagate_expr(inner, vt);
}

fn propagate_expr_match(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
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

fn propagate_expr_lambda(expr: &mut IrExpr, vt: &mut VarTable) {
    let IrExprKind::Lambda { params, body, .. } = &mut expr.kind else { unreachable!() };
    // Sync lambda param types between IR and VarTable — whichever
    // has the concrete type wins. After mono, one or both may still
    // have TypeVar; propagation from call sites resolves them.
    for (vid, ty) in params.iter_mut() {
        if (vid.0 as usize) < vt.len() {
            let vt_ty = &vt.get(*vid).ty;
            if has_typevar(ty) && !has_typevar(vt_ty) {
                // VarTable has concrete type → update IR param
                *ty = vt_ty.clone();
            } else if !has_typevar(ty) && has_typevar(vt_ty) {
                // IR has concrete type → update VarTable
                vt.entries[vid.0 as usize].ty = ty.clone();
            }
        }
    }
    propagate_expr(body, vt);
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
        IrStmtKind::ListSwap { a, b, .. } => { propagate_expr(a, vt); propagate_expr(b, vt); }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => { propagate_expr(end, vt); }
        IrStmtKind::ListCopySlice { len, .. } => { propagate_expr(len, vt); }
        IrStmtKind::Expr { expr } => propagate_expr(expr, vt),
        IrStmtKind::Guard { cond, else_ } => { propagate_expr(cond, vt); propagate_expr(else_, vt); }
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
    }
}
