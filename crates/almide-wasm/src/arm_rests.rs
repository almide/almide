//! #3377 — a NAMED list rest (`[h, ..t]`) as an ARM-SCOPED frame credit.
//!
//! The bind materializes `t` as a fresh block (patterns.rs
//! `emit_pattern_binds`) that no route owns: the unguarded arm releases it
//! once its body has run. Until #3377 every edge that left the arm before
//! that release — a `!` propagation, a guard return, a `return_call` (the
//! recursive walk `[h, ..t] => go(t, …)` itself), a `break` / `continue` —
//! leaked the block.
//!
//! The rest is registered here for the arm body's duration. The exit plan
//! (exit_plan.rs `frame_credits`) counts each registered rest among the
//! frame's credits, so `emit_exit` — the one writer of a frame's releases —
//! releases it on every frame exit, and a loop jump releases the rests bound
//! inside the loop it leaves. The registry is NOT `rc_owned`: the variable's
//! local stays a holder that is not an owner, so the call-argument move /
//! share decisions (calls.rs, writeback_move.rs) and the Bind route's
//! dec-old, which read `rc_owned`, see it exactly as before — an argument
//! `t` is still shared, never moved.
//!
//! The arm's own release on the fall-through path unregisters the rest
//! first, so no edge releases it twice.

use almide_ir::IrPattern;

use crate::emitter::Emitter;
use crate::*;

/// The live arm rests of the frame being lowered, in registration order.
#[derive(Default)]
pub(crate) struct ArmRests {
    /// (local, its list type): bound by an arm whose body is being lowered.
    live: Vec<(u32, SliceTy)>,
    /// `live` entries from this index on were bound inside the innermost
    /// loop body being lowered: the ones its `break` / `continue` leaves.
    loop_floor: usize,
}

impl ArmRests {
    /// The registered rest locals (frame credits of every exit edge).
    pub(crate) fn locals(&self) -> impl Iterator<Item = u32> + '_ {
        self.live.iter().map(|&(l, _)| l)
    }

    /// The list type a registered rest local holds (its typed release).
    pub(crate) fn ty_of(&self, local: u32) -> Option<SliceTy> {
        self.live.iter().find(|&&(l, _)| l == local).map(|&(_, t)| t)
    }

    pub(crate) fn contains(&self, local: u32) -> bool {
        self.live.iter().any(|&(l, _)| l == local)
    }

    /// A chain that walls part-way leaves no rest registered behind it.
    pub(crate) fn mark(&self) -> usize {
        self.live.len()
    }

    pub(crate) fn truncate(&mut self, mark: usize) {
        self.live.truncate(mark);
    }
}

impl Emitter<'_> {
    /// The droppable named rests of `p`, with their locals.
    fn rest_locals(&self, p: &IrPattern) -> Vec<(u32, SliceTy)> {
        let mut vars = Vec::new();
        crate::patterns::named_rests(p, &mut vars);
        vars.iter()
            .filter_map(|v| self.locals.get(v).copied())
            .filter(|&(_, ty)| self.rc_droppable(ty))
            .collect()
    }

    /// An unguarded arm bound `p`: its rests are frame credits until the
    /// arm releases them (`release_arm_rests`).
    pub(crate) fn hold_arm_rests(&mut self, p: &IrPattern) {
        let rests = self.rest_locals(p);
        self.arm_rests.live.extend(rests);
    }

    /// The arm's body has run: it releases each rest block (its value, if
    /// any, holds its own credit — `lower_arm_body` normalizes every value
    /// arm to one). Unregistered first: no later edge releases it again.
    /// A guarded arm never registered its rests and keeps the old leak (a
    /// false guard falls through with the rest already built).
    pub(crate) fn release_arm_rests(&mut self, p: &IrPattern) {
        for (idx, ty) in self.rest_locals(p) {
            if let Some(k) = self.arm_rests.live.iter().rposition(|&(l, _)| l == idx) {
                self.arm_rests.live.remove(k);
            }
            let dec = self.dec_fn_of(ty);
            self.f.instructions().local_get(idx).call(dec);
            if let Some(w) = self.witness.as_mut() {
                w.rest_release(idx);
            }
        }
    }

    /// A loop body opens: rests bound from here on are the ones its jumps
    /// leave. Returns the outer floor for [`Self::close_loop_rests`].
    pub(crate) fn open_loop_rests(&mut self) -> usize {
        std::mem::replace(&mut self.arm_rests.loop_floor, self.arm_rests.live.len())
    }

    pub(crate) fn close_loop_rests(&mut self, outer: usize) {
        self.arm_rests.loop_floor = outer;
    }

    /// A `break` / `continue` leaves every arm opened inside the loop body:
    /// release their rests on the jump edge (the arm's own release after
    /// its body is unreachable on this path). Rests of arms enclosing the
    /// loop stay live — the jump does not leave them.
    pub(crate) fn release_jumped_rests(&mut self) {
        let jumped: Vec<(u32, SliceTy)> = self.arm_rests.live[self.arm_rests.loop_floor.min(self.arm_rests.live.len())..].to_vec();
        for (idx, ty) in jumped.into_iter().rev() {
            let dec = self.dec_fn_of(ty);
            self.f.instructions().local_get(idx).call(dec);
            self.witness_dec(idx);
        }
    }
}
