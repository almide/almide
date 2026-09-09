//! Proofs for reading projections without copying their containing values.
use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

pub(super) fn root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::Borrow { expr: object, mutable: false, .. } => root(object),
        _ => None,
    }
}

fn mentions(e: &IrExpr, v: VarId) -> bool {
    almide_ir::free_vars::free_vars(e, &HashSet::new()).contains(&v)
}

fn reads_binding(e: &IrExpr, var: VarId) -> bool {
    struct Scan { var: VarId, ok: bool }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if !self.ok { return; }
            let direct = |x: &IrExpr| matches!(x.kind, IrExprKind::Var { id } if id == self.var);
            match &e.kind {
                IrExprKind::Borrow { expr, mutable: false, .. }
                | IrExprKind::Clone { expr } if direct(expr) => return,
                IrExprKind::Member { object, .. } if direct(object) => return,
                IrExprKind::Borrow { mutable: true, .. }
                | IrExprKind::Lambda { .. } | IrExprKind::IterChain { .. } if mentions(e, self.var) => {
                    self.ok = false;
                    return;
                }
                IrExprKind::Var { id } if *id == self.var => self.ok = false,
                _ => {}
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. } if *var == self.var => self.ok = false,
                IrStmtKind::FieldAssign { target, .. } | IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. } | IrStmtKind::ListSwap { target, .. }
                | IrStmtKind::ListReverse { target, .. } | IrStmtKind::ListRotateLeft { target, .. }
                    if *target == self.var => self.ok = false,
                IrStmtKind::ListCopySlice { dst, .. } if *dst == self.var => self.ok = false,
                _ => {}
            }
            walk_stmt(self, s);
        }
    }
    let mut scan = Scan { var, ok: true };
    scan.visit_expr(e);
    scan.ok
}

pub(super) fn match_binders(subject: &IrExpr, arms: &[IrMatchArm]) -> Option<HashSet<VarId>> {
    use almide_lang::types::{Ty, constructor::TypeConstructorId};
    if !matches!(subject.ty, Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _)) {
        return None;
    }
    let root = match &subject.kind {
        IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } => root(subject)?,
        IrExprKind::RuntimeCall { symbol, args }
            if symbol.as_str() == "almide_rt_list_get" && args.len() == 2
                && matches!(args[1].kind, IrExprKind::Var { .. } | IrExprKind::LitInt { .. }) => root(&args[0])?,
        _ => return None,
    };
    let mut all = HashSet::new();
    for arm in arms {
        // No arm may mutate or move the source while its fields are borrowed.
        if mentions(&arm.body, root) || arm.guard.as_ref().is_some_and(|g| mentions(g, root)) {
            return None;
        }
        let mut vars = HashSet::new();
        almide_ir::free_vars::collect_pattern_bindings(&arm.pattern, &mut vars);
        for &v in &vars {
            if !reads_binding(&arm.body, v) || arm.guard.as_ref().is_some_and(|g| !reads_binding(g, v)) {
                return None;
            }
        }
        all.extend(vars);
    }
    Some(all)
}

/// An adjacent, single-use projection alias has no observable evaluation gap.
/// Fold it before liveness counting so its match can use the original place.
pub(super) fn fold_bindings(body: &mut IrExpr) {
    use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
    struct Fold;
    impl IrMutVisitor for Fold {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Block { stmts, expr: Some(tail) } = &mut e.kind else { return; };
            let IrExprKind::Match { subject, arms } = &mut tail.kind else { return; };
            let Some(IrStmt { kind: IrStmtKind::Bind { var, value, mutability: Mutability::Let, .. }, .. }) = stmts.last() else { return; };
            if !matches!(subject.kind, IrExprKind::Var { id } if id == *var)
                || match_binders(value, arms).is_none()
                || arms.iter().any(|a| mentions(&a.body, *var) || a.guard.as_ref().is_some_and(|g| mentions(g, *var)))
            { return; }
            let IrStmtKind::Bind { value, .. } = stmts.pop().unwrap().kind else { unreachable!() };
            **subject = value;
        }
    }
    Fold.visit_expr_mut(body);
}
