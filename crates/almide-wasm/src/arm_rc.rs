//! #4278 B1, the arm-internal step: the RC an arm does inside its own
//! window, logged at the instruction that does it.
//!
//! An arm's window (`rcarms`) used to be a trusted class: an `rc_inc` or a
//! release left over inside it was counted as the arm's own. Each such
//! instruction is now an event of the frame's witness — a temporary's
//! (`a-` / `d-`, held by the RC byte check to a call inside the window the
//! event was logged in) on its own object line, which the proven checker
//! balances:
//!
//! - [`Emitter::arm_temp_released`]: a block the arm allocated (a scratch
//!   string, an error text) released before the arm answers — `i d`;
//! - [`Emitter::arm_share_stored`]: a share of a value the arm read out of a
//!   block (+1), handed to a new holder — the arm's result, a container slot,
//!   a callee that consumes its parameter — `a m`.
//!
//! The arm's declaration toward the frame is unchanged — its `ArgMode` per
//! argument and its `Own` result (arm.rs) — and the frame's own events
//! account each argument's credit as before.

use crate::emitter::Emitter;
use crate::SliceTy;

impl Emitter<'_> {
    /// The arm releases a block it allocated itself.
    pub(crate) fn arm_temp_released(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            let o = w.temp_born();
            w.temp_ops(o, "d");
        }
    }

    /// The arm takes a share of a value it read and hands it to a holder.
    pub(crate) fn arm_share_stored(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.view_share_move();
        }
    }

    /// The arm releases the credit a `Retain` argument `arg` moved into it.
    /// A frame variable's credit moved in as that variable's `m` event, which
    /// takes this release; an owned temporary's moved in as its line's `m`
    /// (`im`) with no RC event, so the release is logged here — a
    /// temporary's `d-` held to a release inside this arm's window, with no
    /// further line (the temporary's own line already ends in the move).
    pub(crate) fn arm_retained_released(&mut self, arg: &almide_ir::IrExpr) {
        if self.witness.as_ref().is_some_and(|w| !w.dead()) && self.witness_src_local(arg).is_none() {
            crate::witness::rc::event('d', None);
        }
    }

    /// The arm gives back the `Retain` credit of argument `arg`, parked in
    /// `hold`, on a path that does not store it — [`Emitter::emit_release_hold`]
    /// logged as [`Emitter::arm_retained_released`].
    pub(crate) fn arm_release_hold(&mut self, hold: u32, ty: SliceTy, arg: &almide_ir::IrExpr) {
        self.emit_release_hold(hold, ty);
        if self.elem_is_handle(ty) {
            self.arm_retained_released(arg);
        }
    }

    /// [`Emitter::share_handle_top`], logged as the arm's share handed on.
    pub(crate) fn arm_share_handle_top(&mut self, t: SliceTy) {
        if self.elem_is_handle(t) {
            self.rc_inc_top();
            self.arm_share_stored();
        }
    }
}
