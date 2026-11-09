//! Keep function exits out of a value bind introduced by move-mode lowering.
//! A guard inside `let r = <old body>; ok((r, buffer))` still exits the function.
//! Distribute that continuation through the checked control tree, leaving the
//! guard's exit value alone. No return instruction or new proof rule is needed.
use almide_ir::*;
use almide_lang::types::Ty;

pub(crate) fn expand(ir: &mut IrProgram) {
    for f in &mut ir.functions {
        if f.ret_ty.result_ok_ty().is_some() { tail(&mut f.body, &f.ret_ty); }
    }
}

fn has_guard(e: &IrExpr) -> bool {
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            // These scopes need their own continuation/loop-exit treatment.
            if matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::While { .. } | IrExprKind::ForIn { .. }) { return; }
            almide_ir::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if matches!(s.kind, IrStmtKind::Guard { .. }) { self.0 = true; }
            almide_ir::walk_stmt(self, s);
        }
    }
    let mut scan = Scan(false); scan.visit_expr(e); scan.0
}

fn prepend(s: IrStmt, mut rest: IrExpr) -> IrExpr {
    if let IrExprKind::Block { stmts, .. } = &mut rest.kind {
        stmts.insert(0, s);
        rest
    } else {
        IrExpr { ty: rest.ty.clone(), span: rest.span, def_id: None,
            kind: IrExprKind::Block { stmts: vec![s], expr: Some(Box::new(rest)) } }
    }
}

fn sequence(stmts: &[IrStmt], mut rest: IrExpr, ret: &Ty) -> Option<IrExpr> {
    for s in stmts.iter().rev() {
        match &s.kind {
            IrStmtKind::Guard { cond, else_ } => {
                if else_.ty != *ret || has_guard(cond) || has_guard(else_) { return None; }
                rest = IrExpr { ty: ret.clone(), span: s.span, def_id: None,
                    kind: IrExprKind::If { cond: Box::new(cond.clone()),
                        then: Box::new(rest), else_: Box::new(else_.clone()) } };
            }
            IrStmtKind::Bind { value, .. } | IrStmtKind::Expr { expr: value } if has_guard(value) => {
                rest = continue_value(value, s, &rest, ret)?;
            }
            _ => rest = prepend(s.clone(), rest),
        }
    }
    Some(rest)
}

fn continue_value(value: &IrExpr, stmt: &IrStmt, rest: &IrExpr, ret: &Ty) -> Option<IrExpr> {
    if !has_guard(value) {
        let mut bound = stmt.clone();
        match &mut bound.kind {
            IrStmtKind::Bind { ty, value: target, .. } => {
                if *ty != value.ty { return None; }
                *target = value.clone();
            }
            IrStmtKind::Expr { expr } => *expr = value.clone(),
            _ => return None,
        }
        return Some(prepend(bound, rest.clone()));
    }
    let mut out = value.clone();
    out.ty = ret.clone();
    match &mut out.kind {
        IrExprKind::Block { stmts, expr } => {
            let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: value.span, def_id: None };
            let end = continue_value(expr.as_deref().unwrap_or(&unit), stmt, rest, ret)?;
            return sequence(stmts, end, ret);
        }
        IrExprKind::If { cond, then, else_ } => {
            if has_guard(cond) { return None; }
            **then = continue_value(then, stmt, rest, ret)?;
            **else_ = continue_value(else_, stmt, rest, ret)?;
        }
        IrExprKind::Match { subject, arms } => {
            if has_guard(subject) || arms.iter().any(|a| a.guard.as_ref().is_some_and(has_guard)) { return None; }
            for arm in arms { arm.body = continue_value(&arm.body, stmt, rest, ret)?; }
        }
        _ => return None,
    }
    Some(out)
}

fn tail(e: &mut IrExpr, ret: &Ty) {
    if e.ty != *ret { return; }
    // Only the function-tail chain: moving a function exit into a loop-body
    // value expression would require a separate loop-exit transformation.
    match &mut e.kind {
        IrExprKind::Block { stmts, expr } => {
            for (i, s) in stmts.iter().enumerate() {
                let IrStmtKind::Bind { value, .. } = &s.kind else { continue; };
                if !has_guard(value) { continue; }
                let rest = IrExpr { kind: IrExprKind::Block {
                    stmts: stmts[i + 1..].to_vec(), expr: expr.clone(),
                }, ty: ret.clone(), span: e.span, def_id: None };
                if let Some(mut rewritten) = continue_value(value, s, &rest, ret) {
                    tail(&mut rewritten, ret);
                    *expr = Some(Box::new(rewritten));
                    stmts.truncate(i);
                    return;
                }
            }
            if let Some(t) = expr { tail(t, ret); }
        }
        IrExprKind::If { then, else_, .. } => { tail(then, ret); tail(else_, ret); }
        IrExprKind::Match { arms, .. } => for arm in arms { tail(&mut arm.body, ret); },
        _ => {}
    }
}
