//! The Emitter-side witness hooks (#1696): one per RC-affecting route,
//! each called right where the route emitted its instruction, so the
//! recorder (witness.rs) mirrors the instruction stream event for event.
//!
//! STEP 4 (statement calls + module calls): a native arm declares what it
//! does with each argument (arm.rs `ArgMode`) and what it hands back
//! (`Lowered`); `lower_arg` is the one place an arm's argument crosses
//! into it, so the hook there records the declaration's RC instruction —
//! Borrow: a Var shares nothing, a parked temporary is born and released
//! by the scope (`id`); Retain: a Var takes the `rc_share_guard` +1 and
//! its credit moves into the arm (`am`), a temporary moves in (`im`);
//! Raw: nothing. The wrapper (`lower_module_call`) COUNTS the hooks the
//! arm fired against its argument count: an arm that lowered an argument
//! any other way is unaudited and the frame DECLINES — counted, never
//! under-recorded. A droppable `View` result declines too: its share
//! lands on an object the frame does not track by local, and this
//! phase's object identity is the local map.
//!
//! TRUSTED (not mechanically checked here): an arm that routed every
//! argument through `lower_arg` emits no other rc_inc / dec on those
//! arguments in-frame — the arm.rs doctrine ("Retain IS the share, no
//! arm repeats it"). The runtime helpers' element bookkeeping inside
//! `$block_copy` / `$dec_flat` is the helper's contract, as for phase A.

use crate::arm::{ArgMode, Lowered, Own};
use crate::emitter::Emitter;
use crate::SliceTy;

impl Emitter<'_> {
    fn witness_src_local(&self, e: &almide_ir::IrExpr) -> Option<u32> {
        if let almide_ir::IrExprKind::Var { id } = &e.kind {
            self.locals.get(id).map(|&(l, _)| l)
        } else {
            None
        }
    }

    /// The Bind-route hook (stmts.rs): called right after the local joins
    /// `rc_owned`. Attribution mirrors the instructions the route just
    /// emitted: a certainly-fresh rhs (heap literal) and a Map/Set Var rhs
    /// (which took `$block_copy`) are NEW objects; a List/Str/Bytes Var
    /// rhs took `rc_inc_top`, so the SOURCE object gains a share. A
    /// non-owned call rhs (a native arm's View) declines; anything else
    /// under an armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_bind(&mut self, idx: u32, declared: SliceTy, value: &almide_ir::IrExpr) {
        let src_local = self.witness_src_local(value);
        // Mirrors the route exactly: an OWNED result (fresh, or a user-fn
        // call's handed-over credit, #1986) is a new object; a Map/Set Var
        // took `$block_copy`.
        let owned = self.rc_owned_result(value);
        let is_call = matches!(value.kind, almide_ir::IrExprKind::Call { .. });
        let Some(w) = self.witness.as_mut() else { return };
        if owned || (src_local.is_some() && matches!(declared, SliceTy::Map(..) | SliceTy::Set(_))) {
            w.bind_fresh(idx);
            return;
        }
        match src_local {
            Some(src) if w.bind_alias(idx, src) => {}
            None if is_call => w.decline("bind:view-result"),
            _ => w.poison(),
        }
    }

    /// The call-argument hook (calls.rs, right after `rc_arg_guard`):
    /// a droppable Var argument's object gained a real `rc_inc` and its
    /// credit moves into the callee; a fresh temporary is born and moves.
    /// Non-droppable arguments have no RC site. Anything else under an
    /// armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_arg(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg();
        if !self.rc_droppable(ty) {
            return;
        }
        let src_local = self.witness_src_local(e);
        // Mirrors rc_arg_guard exactly: an OWNED argument (fresh literal
        // or a call result carrying its one credit) is born and moves
        // (`im`); a Var shares and moves (`am`).
        let fresh = self.rc_owned_result(e);
        let Some(w) = self.witness.as_mut() else { return };
        match src_local {
            Some(l) if w.arg_share_move(l) => {}
            None if fresh => w.temp_move(),
            _ => w.poison(),
        }
    }

    /// The call-argument hook for a BORROWED callee param (#2028): a Var
    /// argument has no RC site (the callee holds nothing); a fresh
    /// temporary is born and released by the site (`id`).
    pub(crate) fn witness_arg_borrowed(&mut self, e: &almide_ir::IrExpr, ty: SliceTy, fresh: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg();
        if !self.rc_droppable(ty) {
            return;
        }
        let is_var = matches!(e.kind, almide_ir::IrExprKind::Var { .. });
        let Some(w) = self.witness.as_mut() else { return };
        if fresh {
            w.temp_borrowed();
        } else if !is_var && !matches!(e.kind, almide_ir::IrExprKind::LitStr { .. }) {
            w.poison();
        }
    }

    /// The native-arm argument hook (arm.rs `lower_arg`, after the mode's
    /// instruction): `parked` = the Borrow mode parked an owned temporary
    /// for the scope's release.
    pub(crate) fn witness_module_arg(&mut self, e: &almide_ir::IrExpr, got: SliceTy, mode: ArgMode, parked: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg();
        if !self.rc_droppable(got) {
            return;
        }
        let is_var = matches!(e.kind, almide_ir::IrExprKind::Var { .. });
        let is_static = matches!(e.kind, almide_ir::IrExprKind::LitStr { .. });
        match mode {
            ArgMode::Raw => {}
            ArgMode::Borrow => {
                let Some(w) = self.witness.as_mut() else { return };
                if parked {
                    w.temp_borrowed();
                } else if !is_var && !is_static {
                    w.poison();
                }
            }
            ArgMode::Retain if is_var => self.witness_retain_var(e),
            ArgMode::Retain => {
                let fresh = self.rc_owned_result(e);
                let Some(w) = self.witness.as_mut() else { return };
                if fresh {
                    w.temp_move();
                } else {
                    w.poison();
                }
            }
        }
    }

    /// Retain of a Var mirrors `rc_share_guard`: a cell var shares
    /// nothing (decline — the cell's credit is not this frame's); a
    /// handle-typed local took the real `rc_inc` and its credit moves
    /// into the arm (`am`); a droppable local that is not a handle took
    /// no +1 at all — the arm retains what it did not share: decline.
    fn witness_retain_var(&mut self, e: &almide_ir::IrExpr) {
        let almide_ir::IrExprKind::Var { id } = &e.kind else { return };
        if self.cells.contains(id) {
            self.witness_decline("module-arg:retain-cell");
            return;
        }
        let Some(&(l, vt)) = self.locals.get(id) else {
            self.witness_decline("module-arg:retain-unknown-local");
            return;
        };
        if !self.elem_is_handle(vt) {
            self.witness_decline("module-arg:retain-flat");
            return;
        }
        if let Some(w) = self.witness.as_mut()
            && !w.arg_share_move(l)
        {
            w.poison();
        }
    }

    /// The module-call wrapper's audit (calls_modules.rs): the arm fired
    /// one argument hook per argument, or the frame declines; a droppable
    /// `View` result declines (identity, see the module doc).
    pub(crate) fn witness_module_result(&mut self, name: &str, args: usize, hooks_before: u32, lowered: Option<Lowered>) {
        let Some(w) = self.witness.as_ref() else { return };
        if (w.arg_hooks() - hooks_before) as usize != args {
            self.witness_decline(&format!("module-arm:unaudited:{name}"));
            return;
        }
        if let Some(l) = lowered
            && l.own == Own::View
            && self.rc_droppable(l.ty)
        {
            self.witness_decline(&format!("module-result:view:{name}"));
        }
    }

    /// The statement-position discard (stmts.rs): an owned droppable
    /// result arrived with its one credit and the route released it —
    /// born and released in this frame (`id`).
    pub(crate) fn witness_discard(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.temp_discarded();
        }
    }

    /// The owned-tail hook (func.rs): a droppable tail that needs no
    /// ret-inc — a user-fn call result or a fresh literal — moves its
    /// one credit out of the frame.
    pub(crate) fn witness_tail_owned(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.tail_owned_move();
        }
    }

    /// The heap-Var tail hook (func.rs, after the ret-inc): the bound
    /// Var's object is shared and moves out (`am`); a non-Var borrowed
    /// tail (a native arm's View) declines; anything else is poison.
    pub(crate) fn witness_tail_var(&mut self, tail: &almide_ir::IrExpr) {
        let src = self.witness_src_local(tail);
        let is_call = matches!(tail.kind, almide_ir::IrExprKind::Call { .. });
        let Some(w) = self.witness.as_mut() else { return };
        match src {
            Some(l) if w.ret_move(l) => {}
            None if is_call => w.decline("tail:view-result"),
            _ => w.poison(),
        }
    }

    /// The epilogue hook (func.rs): one `d` per `$dec_flat` emitted.
    pub(crate) fn witness_dec(&mut self, idx: u32) {
        if let Some(w) = self.witness.as_mut()
            && !w.dec_local(idx)
        {
            w.poison();
        }
    }

    /// An emission-time decline: the route this frame took has an RC
    /// site this phase does not record — withdraw the certificate with
    /// a reason the histogram counts (never a silent under-count).
    pub(crate) fn witness_decline(&mut self, reason: &str) {
        if let Some(w) = self.witness.as_mut() {
            w.decline(reason);
        }
    }
}
