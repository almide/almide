//! Rebind a function parameter as a `var` local (#3154).
//!
//! A parameter a closure captures and the fn writes needs the storage a
//! captured-and-written `var` has (the C-319 shared cell), and a parameter has
//! no declaration site to build it at. Both backends give it one the same way:
//! a fresh `var` bound from the parameter as the body's first statement, and
//! every occurrence of the parameter in the body — reads and write targets —
//! renamed onto it. What reads the parameter at the fn's exits (the C-132
//! write-back tuple on the wasm leg, the write-back guard on native) then
//! reads the local, so the caller's place receives the final value.

use crate::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use crate::*;
use almide_lang::types::Ty;

/// Rebind `param` in `body` onto a fresh `var` local allocated in `vt`;
/// returns the local.
pub fn rebind_param_as_var(body: &mut IrExpr, vt: &mut VarTable, param: VarId) -> VarId {
    let info = vt.get(param).clone();
    let local = vt.alloc(info.name, info.ty.clone(), Mutability::Var, info.span);
    vt.entries[local.0 as usize].module_origin = info.module_origin;
    Rename { from: param, to: local }.visit_expr_mut(body);
    prepend_bind(body, local, info.ty, param);
    local
}

/// `var local = param` as the body's first statement.
fn prepend_bind(body: &mut IrExpr, local: VarId, ty: Ty, param: VarId) {
    let read = IrExpr { kind: IrExprKind::Var { id: param }, ty: ty.clone(), span: body.span, def_id: None };
    let bind = IrStmt { kind: IrStmtKind::Bind { var: local, mutability: Mutability::Var, ty, value: read }, span: body.span };
    if let IrExprKind::Block { stmts, .. } = &mut body.kind {
        stmts.insert(0, bind);
        return;
    }
    let inner = std::mem::replace(body, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
    *body = IrExpr {
        ty: inner.ty.clone(),
        span: inner.span,
        def_id: None,
        kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(inner)) },
    };
}

/// Every occurrence of `from` — reads and write targets — becomes `to`.
struct Rename {
    from: VarId,
    to: VarId,
}

impl Rename {
    fn id(&self, v: &mut VarId) {
        if *v == self.from {
            *v = self.to;
        }
    }
}

impl IrMutVisitor for Rename {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Var { id } = &mut e.kind {
            self.id(id);
        }
        walk_expr_mut(self, e);
    }

    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        match &mut s.kind {
            IrStmtKind::Assign { var, .. } | IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => self.id(var),
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. } => self.id(target),
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                self.id(dst);
                self.id(src);
            }
            _ => {}
        }
        walk_stmt_mut(self, s);
    }
}
