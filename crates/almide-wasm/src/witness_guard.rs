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
//!
//! The same derived-select recording carries `let t = if c then a else b`
//! over two borrowed reads (#2755, `bind:view-result`): the Bind route's
//! `rc_inc_top` lands on whichever block the `if` chose, so the share is
//! recorded as a select site after the `if`'s own, each arm aliasing the
//! local to its own source (`witness_bind_select`), and the same `if`
//! handed to a holder (`witness_share_select`).

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
    /// `failed`: the anchor of that `else` (`arm_anchor`, bound once it is
    /// emitted).
    pub(crate) fn witness_verdict_close(&mut self, split: bool, failed: Option<u64>) {
        if split {
            self.witness_branch_arm();
            self.witness_branch_anchor(failed);
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

    /// The sources of `if c then a else b` when each arm is a bound local
    /// (`Local`), a view the arm reads out of one — a slot read, a `!`
    /// payload, a global (`View`) — or itself such an `if` (`Select`, the
    /// nested `if x > 1 then a else b` of an arm). `None` for any other value.
    fn witness_if_sources(&self, value: &almide_ir::IrExpr) -> Option<Src> {
        let tail = crate::rc_ownership::rc_tail(value);
        let almide_ir::IrExprKind::If { then, else_, .. } = &tail.kind else {
            return None;
        };
        let source = |arm: &almide_ir::IrExpr| match self.witness_src_local(arm) {
            Some(l) => Some(Src::Local(l)),
            None if crate::witness_unwrap::is_extraction_view(arm) || self.witness_top_let_ty(arm).is_some() => Some(Src::View),
            None => self.witness_if_sources(arm),
        };
        let site = self.if_sites.get(&(tail as *const _)).copied();
        Some(Src::Select(site, Box::new([source(then)?, source(else_)?])))
    }

    /// A borrowed `if` bound to `idx` (stmts.rs, right after the Bind route's
    /// share): a select site whose arms each take the share on their own
    /// source — a local's block (`a`, aliased as `bind_alias` does), a view
    /// (`bind_view`), or a nested select's. False when the value is not such
    /// an `if`.
    pub(crate) fn witness_bind_select(&mut self, idx: u32, value: &almide_ir::IrExpr) -> bool {
        let Some(sources) = self.witness_if_sources(value) else { return false };
        let Some(w) = self.witness.as_mut() else { return true };
        record_select(w, &sources, &mut |w, src| match src {
            Some(l) if w.bind_alias(idx, l) => {}
            Some(_) => w.poison(),
            None => w.bind_view(idx),
        });
        true
    }

    /// A borrowed `if` handed to a new holder (`witness_share_or_move`, after
    /// the guard's share): a select site whose arms each share their own
    /// source and move it (`am`). False when the value is not such an `if`.
    pub(crate) fn witness_share_select(&mut self, e: &almide_ir::IrExpr) -> bool {
        let Some(sources) = self.witness_if_sources(e) else { return false };
        let Some(w) = self.witness.as_mut() else { return true };
        record_select(w, &sources, &mut |w, src| match src {
            Some(l) if w.arg_share_move(l) => {}
            Some(_) => w.poison(),
            None => w.view_share_move(),
        });
        true
    }
}

/// Where a borrowed `if`'s value comes from, per arm (`witness_if_sources`).
enum Src {
    Local(u32),
    View,
    /// An `if` (its `if_sites` anchor, when `lower_if_arms` recorded one) and
    /// its two arms' sources.
    Select(Option<(u64, [Option<u64>; 2])>, Box<[Src; 2]>),
}

/// Record the select sites of `src`: per arm, its own site nested inside the
/// arm that chose it, and on each leaf the share `leaf` takes (`Some(local)`
/// or `None` for a view). #4631: each arm's events are the +1 past the join
/// on the value that arm chose: held to that shipped arm (or past its join).
fn record_select(w: &mut crate::witness::WitnessRecorder, src: &Src, leaf: &mut dyn FnMut(&mut crate::witness::WitnessRecorder, Option<u32>)) {
    match src {
        Src::Local(l) => leaf(w, Some(*l)),
        Src::View => leaf(w, None),
        Src::Select(site, arms) => {
            select_open(w, site.map(|s| s.0));
            for (k, arm) in arms.iter().enumerate() {
                select_arm(w, site.map(|s| s.0));
                if let Some(a) = site.and_then(|s| s.1[k]) {
                    w.branch_anchor(a);
                }
                record_select(w, arm, leaf);
            }
            w.branch_close();
        }
    }
}

/// A select site over an `if` `lower_if_arms` emitted: anchored to that
/// `if`'s shipped `if` / `else` (its arms are the shipped arms, in order).
fn select_open(w: &mut crate::witness::WitnessRecorder, site: Option<u64>) {
    match site {
        Some(id) => w.branch_open_at(id),
        None => w.branch_open(),
    }
}

fn select_arm(w: &mut crate::witness::WitnessRecorder, site: Option<u64>) {
    match site {
        Some(id) => w.branch_arm_at(id),
        None => w.branch_arm(),
    }
}
