//! A match guard that BINDS a local (#2755, `match-guard:binds`), recorded
//! as the control flow the arm chain emits (patterns.rs `lower_arm_chain`).
//! Split from witness_hooks.rs for the file budget.
//!
//! A guard that binds nothing settles every credit it takes inside itself,
//! so it is recorded on its own arm's path only (witness_gate.rs
//! `match_head_subset`). A local the guard binds outlives it: the block
//! stays in the local until the epilogue (or the next rebind) releases it,
//! on the path that took the arm AND on every path the false guard falls
//! through to. So the arm is recorded as two sites in place of one arm:
//!
//! - the VERDICT site, the `if` the chain emits around the pattern's binds
//!   and the guard: one arm ran them (pattern matched), the other did not
//!   (an irrefutable pattern has no such `if`, so no verdict site);
//! - the SELECT site, the `if` on the verdict: one arm is the body, the
//!   other the rest of the chain (its arms become this site's arms).
//!
//! Every path through the select site then extends a path through the
//! verdict site, so the guard's events reach the fall-through paths too.
//! The product includes infeasible pairs (pattern failed × body); a path set
//! that is a superset of the executable one only adds checks.

use crate::emitter::Emitter;

impl Emitter<'_> {
    /// Does this arm's guard need the split recording?
    pub(crate) fn witness_guard_splits(&self, arm: &almide_ir::IrMatchArm) -> bool {
        self.witness.is_some() && arm.guard.as_ref().is_some_and(crate::witness::binds_a_local)
    }

    /// The verdict site opens: its first arm runs the binds and the guard.
    pub(crate) fn witness_verdict_open(&mut self, split: bool) {
        if split {
            self.witness_branch_open();
            self.witness_branch_arm();
        }
    }

    /// The verdict site's other arm (the pattern did not match: the `else`
    /// that pushes 0) and its join.
    pub(crate) fn witness_verdict_close(&mut self, split: bool) {
        if split {
            self.witness_branch_arm();
            self.witness_branch_close();
        }
    }

    /// The select site opens with the body's arm; the rest of the chain
    /// opens its own arms in it (`lower_arm_chain` begins with one).
    pub(crate) fn witness_select_open(&mut self, split: bool) {
        if split {
            self.witness_branch_open();
            self.witness_branch_arm();
        }
    }

    /// The select site joins.
    pub(crate) fn witness_select_close(&mut self, split: bool) {
        if split {
            self.witness_branch_close();
        }
    }
}
