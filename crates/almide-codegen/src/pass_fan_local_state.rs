//! Analysis-only view of invocation-local state for explicit fan purity.
//! This IR is never emitted. Loops retain all potentially effectful children;
//! only writes to bindings owned by this invocation are erased from the view.
use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::Ty;

pub(super) fn view(function: &IrFunction) -> Option<IrExpr> {
    let params: HashSet<_> = function.params.iter().map(|p| p.var).collect();
    if !free_vars::free_vars(&function.body, &params).is_empty() { return None; }
    // Heap parameters can alias caller-owned storage. Keep their original,
    // stricter body rather than granting local mutation admission.
    if !function.params.iter().all(|p| scalar(&p.ty)) {
        return Some(function.body.clone());
    }
    let mut body = function.body.clone();
    let locals = free_vars::bound_vars(&body);
    View { locals }.visit_expr_mut(&mut body);
    Some(body)
}

fn scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit
        | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float32 | Ty::Float64)
}

struct View { locals: HashSet<VarId> }

impl IrMutVisitor for View {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        match &mut e.kind {
            IrExprKind::Borrow { expr, mutable, .. } => {
                if matches!(expr.kind, IrExprKind::Var { id } if self.locals.contains(&id)) {
                    *mutable = false;
                }
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                e.kind = IrExprKind::Block {
                    stmts: std::mem::take(body), expr: Some(std::mem::take(iterable)),
                };
            }
            IrExprKind::While { cond, body } => {
                e.kind = IrExprKind::Block {
                    stmts: std::mem::take(body), expr: Some(std::mem::take(cond)),
                };
            }
            IrExprKind::Break | IrExprKind::Continue => e.kind = IrExprKind::Unit,
            _ => {}
        }
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        let values = match &mut s.kind {
            IrStmtKind::Assign { var, value } if self.locals.contains(var) => vec![std::mem::take(value)],
            IrStmtKind::FieldAssign { target, value, .. } if self.locals.contains(target) => vec![std::mem::take(value)],
            IrStmtKind::IndexAssign { target, index, value } if self.locals.contains(target) =>
                vec![std::mem::take(index), std::mem::take(value)],
            IrStmtKind::MapInsert { target, key, value } if self.locals.contains(target) =>
                vec![std::mem::take(key), std::mem::take(value)],
            IrStmtKind::ListSwap { target, a, b } if self.locals.contains(target) =>
                vec![std::mem::take(a), std::mem::take(b)],
            IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end }
                if self.locals.contains(target) => vec![std::mem::take(end)],
            IrStmtKind::ListCopySlice { dst, len, .. } if self.locals.contains(dst) => vec![std::mem::take(len)],
            _ => return,
        };
        let ty = Ty::Tuple(values.iter().map(|e| e.ty.clone()).collect());
        s.kind = IrStmtKind::Expr { expr: IrExpr {
            kind: IrExprKind::Tuple { elements: values }, ty, span: s.span, def_id: None,
        } };
    }
}
