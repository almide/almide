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
                let mutates = matches!(
                    (module.as_str(), func.as_str()),
                    ("list", "push" | "pop" | "clear")
                        | ("map", "insert")
                        | ("string", "push")
                        // bytes' in-place writers were MISSING here: a
                        // captured Bytes var mutated through them was never
                        // cell-classified, took the env value-copy path, and
                        // printed a silently wrong value (the develop
                        // wasm_runtime catch at the commissioning switchover).
                        | ("bytes", "push" | "set_at" | "set_f32_le" | "set_f64_le" | "fill" | "clear")
                );
                if mutates
                    && let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind)
                {
                    self.mutated.insert(*id);
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Assign { var, .. } => {
                self.mutated.insert(*var);
            }
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => {
                self.mutated.insert(*target);
            }
            _ => {}
        }
        walk_stmt(self, s);
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
        }
        let mut sc = Scan { locals: self.locals, params, out: Vec::new() };
        almide_ir::visit::IrVisitor::visit_expr(&mut sc, body);
        sc.out
    }
}
