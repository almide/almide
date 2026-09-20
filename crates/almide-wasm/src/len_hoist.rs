//! #2319: the element COUNT a bounds check reads is loop-invariant for a
//! list the loop never rebinds, so it is loaded once before the loop instead
//! of on every access.
//!
//! `xs[i]` checks `i >=u len(xs)/stride`, and `len` is a load from the
//! block. Cranelift will not hoist that load out of the loop (a load may
//! alias a store), so spectralnorm's inner loop reloaded and divided the
//! length 57.6M times — 3 ms of its 42.
//!
//! WHAT CHANGES A COUNT: only a statement that rebinds the variable —
//! `Assign`, a fresh `Bind` of the same id, or a destructure binding it.
//! `list.push` and friends reach the variable through the C-132 move-mode
//! write-back, which IS an `Assign`, so they are covered. An element store
//! (`IndexAssign`) and the peephole list ops keep the length (a COW copy
//! changes the block ADDRESS, never the count, and the address is re-read at
//! every access — only the count is cached). A Map is a different statement
//! (`MapInsert`) and a different access path.
//!
//! WHAT THE SCAN REFUSES, conservatively: a variable passed to any call
//! inside the loop, and every candidate when the loop contains a lambda —
//! the point is one measured inner loop, not a general invariance proof.

use std::collections::BTreeSet;

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

#[derive(Default)]
pub(crate) struct Scan {
    /// Vars read as `v[i]` or written as `v[i] = …` — the candidates.
    indexed: BTreeSet<VarId>,
    /// Vars the loop rebinds, or hands to a call, or a lambda could reach.
    disqualified: BTreeSet<VarId>,
    /// A lambda in the loop refuses every candidate (it can capture a list
    /// and be called anywhere, including through a stored closure).
    any_lambda: bool,
}

impl Scan {
    /// The vars whose count is loop-invariant, in VarId order (the emission
    /// order must be deterministic — the goldens compare bytes).
    pub(crate) fn invariant(self) -> Vec<VarId> {
        if self.any_lambda {
            return Vec::new();
        }
        self.indexed.difference(&self.disqualified).copied().collect()
    }
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::IndexAccess { object, .. } => {
                if let IrExprKind::Var { id } = &object.kind {
                    self.indexed.insert(*id);
                }
            }
            IrExprKind::Lambda { .. } => self.any_lambda = true,
            IrExprKind::Call { args, .. } => {
                for a in args {
                    if let IrExprKind::Var { id } = &a.kind {
                        self.disqualified.insert(*id);
                    }
                }
            }
            _ => {}
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Assign { var, .. } | IrStmtKind::Bind { var, .. } => {
                self.disqualified.insert(*var);
            }
            IrStmtKind::IndexAssign { target, .. } => {
                self.indexed.insert(*target);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }

    fn visit_pattern(&mut self, pat: &almide_ir::IrPattern) {
        // Any binder inside the loop (a destructure, a match arm) rebinds;
        // the walker reaches every sub-pattern, so the two binding forms are
        // all this arm needs.
        match pat {
            almide_ir::IrPattern::Bind { var, .. } | almide_ir::IrPattern::As { var, .. } => {
                self.disqualified.insert(*var);
            }
            _ => {}
        }
        almide_ir::visit::walk_pattern(self, pat);
    }
}

/// Scan a loop (its condition and its body) for lists whose count is
/// invariant across it.
pub(crate) fn invariant_counts(cond: Option<&IrExpr>, body: &[IrStmt]) -> Vec<VarId> {
    let mut scan = Scan::default();
    if let Some(c) = cond {
        scan.visit_expr(c);
    }
    for st in body {
        scan.visit_stmt(st);
    }
    scan.invariant()
}

impl crate::emitter::Emitter<'_> {
    /// Load the element count of every list the loop never rebinds into a
    /// held local, before the loop. Returns the vars it added, for
    /// `drop_hoisted_counts` to undo at the loop's end.
    pub(crate) fn hoist_invariant_counts(
        &mut self,
        cond: Option<&IrExpr>,
        body: &[IrStmt],
    ) -> Result<Vec<VarId>, crate::EmitError> {
        let mut added = Vec::new();
        for v in invariant_counts(cond, body) {
            if self.hoisted_counts.contains_key(&v) {
                continue; // an enclosing loop already hoisted it
            }
            let Some(&(slot, crate::SliceTy::List(h))) = self.locals.get(&v) else {
                continue; // a global, a range bind, or not a list
            };
            if self.cells.contains(&v) {
                continue; // read through a cell address, not a plain handle
            }
            let stride = self.types.el(h).slot_size() as i32;
            let count = self.hold_i64()?;
            let mut i = self.f.instructions();
            i.local_get(slot).i32_load(crate::len_memarg()).i32_const(stride).i32_div_u();
            i.i64_extend_i32_u().local_set(count);
            self.hoisted_counts.insert(v, count);
            added.push(v);
        }
        Ok(added)
    }

    /// Release the hoisted counts of one loop, innermost hold first.
    pub(crate) fn drop_hoisted_counts(&mut self, added: Vec<VarId>) {
        for v in added.into_iter().rev() {
            self.hoisted_counts.remove(&v);
            self.release_i64();
        }
    }

    /// The count of `v`, when a loop hoisted it.
    pub(crate) fn hoisted_count_of(&self, v: VarId) -> Option<u32> {
        self.hoisted_counts.get(&v).copied()
    }
}
