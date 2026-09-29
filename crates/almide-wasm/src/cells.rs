//! C-319 shared-cell classification: a var that is BOTH referenced from
//! inside some lambda AND mutated anywhere in the fn lives in a one-slot
//! heap CELL — the local (in the fn and in every capturing closure)
//! holds the cell's address, reads load through it, writes store through
//! it, so mutation is visible in BOTH directions (the interp's
//! scope-by-reference capture, mirrored). The capture side is an
//! OVER-approximation (every Var id occurring inside a lambda subtree):
//! a cell for a var nobody actually captures is observably identical,
//! only slower — never unsound. For-in loop vars are excluded: the
//! interp BINDS them fresh per iteration (a new cell each time), so the
//! loop's own advancement is not a mutation of one shared cell.

use std::collections::{HashMap, HashSet};

use crate::SliceTy;

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

#[derive(Default)]
struct Scan {
    in_lambda: u32,
    captured: HashSet<VarId>,
    mutated: HashSet<VarId>,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Lambda { .. } => {
                self.in_lambda += 1;
                walk_expr(self, e);
                self.in_lambda -= 1;
                return;
            }
            IrExprKind::Var { id } if self.in_lambda > 0 => {
                self.captured.insert(*id);
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                // Which args a stdlib call writes comes from the callee's
                // DECLARATION (`mut` params), the source native's borrow
                // inference and the shared optimizer read too. A hand list
                // of mutators lived here and drifted twice: bytes' writers
                // were missing (the develop wasm_runtime catch), and then
                // every bytes writer but six still was — a captured Bytes
                // written through `bytes.set_u8` took the env value-copy
                // path and the write was lost (#2951).
                for k in almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str()).unwrap_or_default() {
                    if let Some(IrExprKind::Var { id }) = args.get(k).map(|a| &a.kind) {
                        self.mutated.insert(*id);
                    }
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        // A write target names the var without a `Var` read: inside a
        // lambda it is a capture too (`() => { xs[5] = 1 }` reads nothing).
        if let Some(v) = write_target(s) {
            self.mutated.insert(v);
            if self.in_lambda > 0 {
                self.captured.insert(v);
            }
        }
        walk_stmt(self, s);
    }
}

/// The var a write statement stores into, if it is one.
fn write_target(s: &IrStmt) -> Option<VarId> {
    match &s.kind {
        IrStmtKind::Assign { var, .. } => Some(*var),
        IrStmtKind::IndexAssign { target, .. }
        | IrStmtKind::MapInsert { target, .. }
        | IrStmtKind::FieldAssign { target, .. } => Some(*target),
        _ => None,
    }
}

/// Vars needing cell storage in this fn body.
pub(crate) fn cell_vars_of(body: &IrExpr) -> HashSet<VarId> {
    let mut s = Scan::default();
    s.visit_expr(body);
    s.captured.intersection(&s.mutated).copied().collect()
}

impl crate::emitter::Emitter<'_> {
    /// The lambda body's captured OUTER locals (VarIds are unique within
    /// a function context, so any Var resolving through the enclosing
    /// locals map that is not a lambda param is a capture).
    pub(crate) fn captured_vars(
        &self,
        params: &std::collections::HashSet<VarId>,
        body: &IrExpr,
    ) -> Vec<(VarId, SliceTy)> {
        struct Scan<'x> {
            locals: &'x HashMap<VarId, (u32, SliceTy)>,
            params: &'x std::collections::HashSet<VarId>,
            out: Vec<(VarId, SliceTy)>,
        }
        impl almide_ir::visit::IrVisitor for Scan<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind
                    && !self.params.contains(id)
                    && let Some(&(_, ty)) = self.locals.get(id)
                    && !self.out.iter().any(|(v, _)| v == id)
                {
                    self.out.push((*id, ty));
                }
                almide_ir::visit::walk_expr(self, e);
            }
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let Some(id) = write_target(s)
                    && !self.params.contains(&id)
                    && let Some(&(_, ty)) = self.locals.get(&id)
                    && !self.out.iter().any(|(v, _)| *v == id)
                {
                    self.out.push((id, ty));
                }
                almide_ir::visit::walk_stmt(self, s);
            }
        }
        let mut sc = Scan { locals: self.locals, params, out: Vec::new() };
        almide_ir::visit::IrVisitor::visit_expr(&mut sc, body);
        sc.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_types::types::Ty;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Unknown, span: None, def_id: None }
    }

    /// The #2951 GATE, enumerated from the stdlib declarations rather than a
    /// hand list: a var a closure captures and that ANY stdlib call writes
    /// through a declared `mut` param gets a shared cell. The list this
    /// replaced covered six of the fifty bytes writers.
    #[test]
    fn every_stdlib_mut_write_to_a_captured_var_makes_a_cell() {
        let mut missed = Vec::new();
        for (module, func, idxs) in almide_ir::mut_args::stdlib_mut_fns() {
            let arity = idxs.iter().max().copied().unwrap_or(0) + 1;
            let args: Vec<IrExpr> = (0..arity)
                .map(|i| if i == idxs[0] { e(IrExprKind::Var { id: VarId(0) }) } else { e(IrExprKind::LitInt { value: 0 }) })
                .collect();
            let call = e(IrExprKind::Call {
                target: CallTarget::Module { module: almide_base::intern::sym(module), func: almide_base::intern::sym(&func), def_id: None },
                args,
                type_args: vec![],
            });
            let lambda = e(IrExprKind::Lambda { params: vec![], body: Box::new(e(IrExprKind::Var { id: VarId(0) })), lambda_id: None });
            let body = e(IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Expr { expr: lambda }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None },
                ],
                expr: None,
            });
            if !cell_vars_of(&body).contains(&VarId(0)) {
                missed.push(format!("{module}.{func}"));
            }
        }
        assert!(missed.is_empty(), "a captured var written through these got no cell: {missed:?}");
    }
}
