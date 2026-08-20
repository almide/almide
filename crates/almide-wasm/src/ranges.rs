//! Bound-range deferral (#1400 / C-238): a `let`-bound range whose EVERY
//! use is a `for-in` head never materializes — the loop counts between
//! two i64 locals. The load-bearing cell is 4294967295 iterations
//! (range_bind_huge.almd): a materializing leg needs ~32 GB; the
//! counting leg needs two locals. Any other reading of the var — an
//! index, a `list.len`, a reassignment, a capture into a lambda —
//! DISQUALIFIES the bind, and the value-position `Range` arm in
//! emitter.rs materializes the real `List[Int]` block instead (with the
//! C-197 `out of memory` die past the wasm leg's own structural bound —
//! success between the two legs' bounds is the contracted divergence,
//! runtime/rs list.rs ratified A 2026-08-17).

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

#[derive(Default)]
struct RangeScan {
    /// Bind var → inclusive flag, for binds whose initializer is a Range.
    candidates: HashMap<VarId, bool>,
    /// Var-expression occurrences, all positions.
    total: HashMap<VarId, usize>,
    /// Occurrences that are exactly a `for-in` head (outside lambdas).
    heads: HashMap<VarId, usize>,
    /// Reassigned or rebound vars — never deferred.
    tainted: HashSet<VarId>,
    lambda_depth: usize,
}

impl IrVisitor for RangeScan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Var { id } => {
                *self.total.entry(*id).or_default() += 1;
                if self.lambda_depth > 0 {
                    // A capture would need a real block to cross the
                    // closure boundary.
                    self.tainted.insert(*id);
                }
            }
            IrExprKind::Lambda { .. } => {
                self.lambda_depth += 1;
                walk_expr(self, e);
                self.lambda_depth -= 1;
                return;
            }
            IrExprKind::ForIn { iterable, .. } => {
                if self.lambda_depth == 0
                    && let IrExprKind::Var { id } = &iterable.kind
                {
                    *self.heads.entry(*id).or_default() += 1;
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                if let IrExprKind::Range { inclusive, .. } = &value.kind {
                    if self.candidates.insert(*var, *inclusive).is_some() {
                        // Rebound (shadowing shares the VarId only if the
                        // front end reuses it — either way, not deferrable).
                        self.tainted.insert(*var);
                    }
                } else {
                    self.tainted.insert(*var);
                }
            }
            IrStmtKind::Assign { var, .. } => {
                self.tainted.insert(*var);
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

/// Vars deferrable to the counting path: bound ONCE to a syntactic
/// range, never assigned, and every occurrence is a for-in head.
pub(crate) fn deferred_ranges_of(body: &IrExpr) -> HashMap<VarId, bool> {
    let mut scan = RangeScan::default();
    scan.visit_expr(body);
    scan.candidates
        .into_iter()
        .filter(|(v, _)| {
            !scan.tainted.contains(v)
                && scan.total.get(v).copied().unwrap_or(0)
                    == scan.heads.get(v).copied().unwrap_or(0)
        })
        .collect()
}
