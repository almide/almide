//! One call argument under the callee's declared convention — split from
//! calls.rs for the file budget. The mode it is handed over with is also the
//! call-mode witness's (witness_modes.rs, #2758).

use almide_ir::{IrExpr, IrExprKind};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// One already-lowered argument under the callee's declared convention
    /// (param_borrow.rs, #2028) — the Named and the registry route share
    /// it, so the two cannot disagree with the ONE `param_owned` table.
    /// `owned_pos` = the callee owns this param (releases it at its exit
    /// plan). Returns true when a borrowed position forbids a
    /// `return_call` at this site (`no_transfer`).
    pub(crate) fn lower_conv_arg(
        &mut self,
        a: &IrExpr,
        want: SliceTy,
        owned_pos: bool,
        true_tail: bool,
        must_transfer: bool,
    ) -> Result<bool, EmitError> {
        let is_static = matches!(a.kind, IrExprKind::LitStr { .. });
        let fresh = self.rc_droppable(want) && self.rc_owned_result(a) && !is_static;
        // Past a true tail site only a value THIS frame does not own
        // survives the exit plan: a param it borrows itself, or a pool
        // static (param_borrow.rs `tail_safe_arg`).
        let tail_safe = !true_tail
            || is_static
            || matches!(&a.kind, IrExprKind::Var { id }
                if self.locals.get(id).is_some_and(|&(idx, _)| {
                    idx < self.rc_param_ceiling && !self.rc_frame_params.contains(&idx)
                }));
        // A borrowed POSITION: droppable, and not owned by the callee (a
        // scalar param has no convention at all).
        let borrowed_pos = self.rc_droppable(want) && !owned_pos;
        let no_transfer = borrowed_pos && !tail_safe && !must_transfer;
        let moved = !borrowed_pos || (must_transfer && !tail_safe);
        self.modes_arg(want, moved);
        if moved {
            // RC-3 callee-owned args: a borrowed droppable argument gets
            // +1 here, the callee's epilogue decs its params — the pair
            // keeps a mut-param callee's realloc-free honest (rc reflects
            // both holders).
            self.rc_arg_guard(a, want);
            self.witness_arg(a, want);
        } else {
            // A borrowed param (#2028): a Var passes as is; an owned
            // temporary is parked for release after the call (a string
            // literal is a pool static: nothing to release).
            if fresh {
                if self.borrowed_temps.len() as u32 >= crate::emitter::BORROW_POOL {
                    return Err(EmitError::Unsupported("borrow-depth".into()));
                }
                let h = self.borrow_base + self.borrowed_temps.len() as u32;
                self.f.instructions().local_tee(h);
                self.borrowed_temps.push((h, want));
            }
            self.witness_arg_borrowed(a, want, fresh);
        }
        Ok(no_transfer)
    }

    /// A droppable argument's hand-over mode, for the call-mode witness.
    pub(crate) fn modes_arg(&self, want: SliceTy, moved: bool) {
        if self.rc_droppable(want) {
            crate::witness::modes::site_arg(moved);
        }
    }
}
