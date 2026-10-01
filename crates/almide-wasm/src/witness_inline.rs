//! The inlined-callback witness hooks of the native arms that run a literal
//! callback AT MOST ONCE (#2755 / #2758): the option / result combinators
//! (sums.rs). Split from witness_hooks.rs for the file budget.
//!
//! Such an arm is a branch site of its own: one arm runs the callback body
//! (lowered inline, its sites the ordinary hooks; each param a VIEW of the
//! payload it is loaded from), the other passes the subject through or
//! shares its payload. The body's value is settled by the share guard the
//! arm emits after it, which `witness_store` mirrors; what the arm does with
//! a RETAINED subject (hand it back, release it) is the arm's own credit,
//! recorded at `lower_arg` as the move into the arm.

use crate::emitter::Emitter;
use crate::SliceTy;

impl Emitter<'_> {
    /// The callback's arm of the site opens (after `hof_lambda` mapped the
    /// params): the callback node is the arm's argument, hooked here for
    /// the module-call audit (it never passes `lower_arg`).
    pub(crate) fn witness_once_open(&mut self, cb: &almide_ir::IrExpr) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(cb as *const almide_ir::IrExpr as usize);
        w.branch_open();
        w.branch_arm();
        let almide_ir::IrExprKind::Lambda { params, .. } = &cb.kind else { return };
        for (var, _) in params {
            if let Some(&(idx, ty)) = self.locals.get(var) {
                self.witness_view_local(idx, ty);
            }
        }
    }

    /// The site's other arm begins.
    pub(crate) fn witness_once_arm(&mut self) {
        self.witness_branch_arm();
    }

    /// The site joins.
    pub(crate) fn witness_once_close(&mut self) {
        self.witness_branch_close();
    }

    /// A payload read out of the subject and handed out with a share
    /// (`share_handle_top`): a view that takes the +1 and moves it to the
    /// arm's owned result (`am`). A droppable payload that is not a handle
    /// takes no +1 the owned result could stand on: decline.
    pub(crate) fn witness_payload_share(&mut self, t: SliceTy) {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return;
        }
        if !self.elem_is_handle(t) {
            self.witness_decline("payload-share:flat");
            return;
        }
        if let Some(w) = self.witness.as_mut() {
            w.view_share_move();
        }
    }
}
