//! Proofs for reading projections without copying their containing values.
use std::collections::HashSet;
use almide_ir::*;
use super::use_kind::{ExplicitBorrows, Site, UseSites};

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

/// Does every occurrence of `var` in `e` read it through a borrow the walker
/// renders identically for a `&T` binding — a shared `Borrow`, a `Clone`, a
/// field read? A bare occurrence, a write, anything under a `&mut`, and any
/// use a closure or fused chain captures says no.
fn reads_binding(e: &IrExpr, var: VarId) -> bool {
    UseSites::of_expr(e, Site::Result, &ExplicitBorrows).of(var).all(|u| {
        u.depth == 0 && !u.in_chain && !u.in_mut
            && matches!(u.site, Site::Borrow { mutable: false } | Site::Clone | Site::Member)
    })
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
