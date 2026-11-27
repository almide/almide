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

    /// A non-rest local that holds a credit only the exit edges must
    /// release (a metered fold's accumulator, writeback_move.rs): the same
    /// exit-plan treatment as a rest, scoped by [`Self::mark`] /
    /// [`Self::truncate`] around the body that may exit.
    pub(crate) fn hold_local(&mut self, local: u32, ty: SliceTy) {
        self.live.push((local, ty));
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

    /// An arm bound `p`: its rests are frame credits until the arm releases
    /// them (`release_arm_rests`). A guarded arm binds before its guard runs
    /// (the guard may read a rest), so its rests are held from there on.
    pub(crate) fn hold_arm_rests(&mut self, p: &IrPattern) {
        let rests = self.rest_locals(p);
        self.arm_rests.live.extend(rests);
    }

    /// #2971: the guard of an arm that bound rests has left its Bool on the
    /// stack. A FALSE guard falls through to the next arm with the rests
    /// built, so that edge releases them and empties their locals; the guard
    /// becomes the verdict through a two-arm `if` the recorder records as a
    /// site (the true arm settles nothing, the body's arm releases the rests
    /// after it; the false arm's `d`, after which the local holds nothing, so
    /// the body's release on a path the product of sites only adds is a
    /// release of NULL). Before this, every guarded arm's rest leaked — on a
    /// false guard AND after a body that ran.
    pub(crate) fn settle_guard_rests(&mut self, p: &IrPattern) {
        let rests = self.rest_locals(p);
        if rests.is_empty() {
            return;
        }
        self.f.instructions().if_(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32));
        let then_at = self.arm_anchor_here();
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_branch_anchor(then_at);
        self.f.instructions().i32_const(1).else_();
        let else_at = self.arm_anchor_here();
        self.witness_branch_arm();
        self.witness_branch_anchor(else_at);
        for (idx, ty) in rests {
            let dec = self.dec_fn_of(ty);
            self.f.instructions().local_get(idx).call(dec).i32_const(0).local_set(idx);
            if let Some(w) = self.witness.as_mut() {
                if !w.dec_local(idx) {
                    w.poison();
                }
                w.empty_local(idx);
            }
        }
        self.witness_branch_close();
        self.f.instructions().i32_const(0).end();
    }

    /// A REFUTABLE guarded arm that binds rests carries its verdict in a
    /// local, 0 before the pattern test and the guard's (settled) verdict
    /// once the pattern matched, which the select `if` reads: the path on
    /// which the pattern did not match then holds a known 0 and never enters
    /// the body, so the RC byte check's last-read walk (anchor-check
    /// rc/after.rs, which decides an `if` only by a local it knows) does not
    /// pair the body's read of a rest with a release from a previous
    /// iteration. `None` (the verdict stays on the stack) for any other arm.
    pub(crate) fn rest_verdict_local(&mut self, p: &IrPattern) -> Result<Option<u32>, EmitError> {
        if self.rest_locals(p).is_empty() {
            return Ok(None);
        }
        let v = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(v);
        Ok(Some(v))
    }

    /// The arm's body has run: it releases each rest block (its value, if
    /// any, holds its own credit — `lower_arm_body` normalizes every value
    /// arm to one). Unregistered first: no later edge releases it again.
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
