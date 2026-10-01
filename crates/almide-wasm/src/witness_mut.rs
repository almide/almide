//! The mut-receiver and inlined-thunk witness hooks (#2755), split from
//! witness_hooks.rs for the file budget.
//!
//! A mut RECEIVER (`list.push(xs, v)`'s `xs`, `bytes.set_u8(b, …)`'s `b`,
//! `string.push(h.f, s)`'s `h`) never passes `lower_arg`: the arm reads the
//! var's slot and writes the result back. Every write-back site records the
//! rebind ([`Emitter::witness_mut_rebind`]) and notes the var for the
//! module-call audit, so a receiver the arm rebound counts as hooked
//! (witness_hooks.rs `witness_module_result`).

use crate::emitter::Emitter;
use crate::SliceTy;

/// A mut RECEIVER's identity for the module-call audit (#2755): the var a
/// write-back rebound, tagged so it can never equal a node address (an
/// IrExpr is aligned and lives far below the top bit).
pub(crate) fn var_key(id: almide_ir::VarId) -> usize {
    (1usize << (usize::BITS - 1)) | id.0 as usize
}

impl Emitter<'_> {
    /// #2755: a mut receiver's slot was rebound (emitter_vars.rs / list_mut.rs
    /// / bytes.rs / map_inplace.rs): the copy-on-write read (`$cow`), a
    /// realloc-on-growth helper's result (`$list_push`, `$bytes_push`), a
    /// functional rebuild written back (`emit_rebind_mut_var_fresh`), the
    /// shrunken copy of a pop. Each takes the var's one credit on the block
    /// it held (`d`: `$cow` releases it when it copies, the helper frees the
    /// outgrown block or the route's `$dec` drops it) and leaves the var
    /// holding one credit on the block now in its slot (`i`) — the same
    /// block when the write landed in place, which the two events then
    /// account as a hand-over through the helper. An in-place write after
    /// the read records nothing. A global or a C-319 cell is an OUTER holder
    /// the frame borrows ([`Emitter::witness_holder`]): its rebind is the
    /// same `d` / `i`, on the holder's pseudo local.
    pub(crate) fn witness_mut_rebind(&mut self, id: almide_ir::VarId, global: bool) {
        if self.witness.is_none() {
            return;
        }
        let local = self.witness_holder(id, global);
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(var_key(id));
        match local {
            Some(l) if w.assign(l, true, None) => {}
            _ => w.poison(),
        }
    }

    /// The local a write to `id` rebinds: a frame local; or, for a top-level
    /// global or a C-319 cell, the pseudo local naming that OUTER holder,
    /// marked as borrowed by the frame (witness.rs `outer_holder`). Pseudo
    /// locals live far above any wasm local index.
    pub(crate) fn witness_holder(&mut self, id: almide_ir::VarId, global: bool) -> Option<u32> {
        let holder = if global {
            0xC000_0000 | self.globals.get(&(self.var_space, id))?.0
        } else if self.cells.contains(&id) {
            0xA000_0000 | self.locals.get(&id)?.0
        } else {
            return self.locals.get(&id).map(|&(l, _)| l);
        };
        self.witness.as_mut()?.outer_holder(holder);
        Some(holder)
    }

    /// #2755: the copy-on-write field write (list_mut.rs `field_assign_with`)
    /// rebound its root var — when it settled the old block itself: a value
    /// that `spends` the var's credit is the C-132 write-back's.
    pub(crate) fn witness_field_rebind(&mut self, id: almide_ir::VarId, global: bool, spends: bool, root: SliceTy) {
        if !spends && self.rc_droppable(root) {
            self.witness_mut_rebind(id, global);
        }
    }

    /// #2755: a mut receiver read WITHOUT the copy-on-write — a parameter,
    /// whose writes stay caller-visible (`emit_read_mut_var_cow`). The read
    /// moves no credit; an in-place write through it records nothing, and a
    /// helper that answers with another block is a rebind of its own.
    pub(crate) fn witness_mut_read(&mut self, id: almide_ir::VarId, global: bool) {
        if global || self.cells.contains(&id) {
            return;
        }
        if let Some(w) = self.witness.as_mut() {
            w.note_arg(var_key(id));
        }
    }

    /// #2755: an argument an arm consumes WITHOUT lowering it as a value —
    /// `fan.any { … }`'s thunk list, whose bodies the arm inlines (each one's
    /// sites are the ordinary hooks).
    pub(crate) fn witness_inline_arg(&mut self, e: &almide_ir::IrExpr) {
        if let Some(w) = self.witness.as_mut() {
            w.note_arg(e as *const almide_ir::IrExpr as usize);
        }
    }

    /// #2755: one arm of the `fan.any { … }` block form (fan.rs), right after
    /// its Result carrier was born ([`Self::witness_fan_carrier`]): a branch
    /// site whose first arm is the winner — the carrier leaves as the call's
    /// owned result (`m`) — and whose second is the loser — released (`d`),
    /// with the arms after it emitted inside it. The fan closes every site
    /// it opened (`true` here) after the all-fail result.
    pub(crate) fn witness_any_arm(&mut self, c: Option<u32>) -> bool {
        let (Some(w), Some(o)) = (self.witness.as_mut(), c) else { return false };
        w.branch_open();
        w.branch_arm();
        w.temp_ops(o, "m");
        w.branch_arm();
        w.temp_ops(o, "d");
        true
    }

    /// #2755: a PURE arm of `fan.any { … }` wins: its bare value moves into
    /// the fresh ok block without a share — an owned value moves (`im`), a
    /// borrowed one would need the share the route does not take (decline).
    /// The arms after it are dead code the recorder would log as live.
    pub(crate) fn witness_any_pure_arm(&mut self, e: &almide_ir::IrExpr, t: SliceTy, last: bool) {
        let moves = self.rc_droppable(t);
        let owned = self.rc_owned_result(e);
        let Some(w) = self.witness.as_mut() else { return };
        match (last, moves, owned) {
            (false, _, _) => w.decline("fan:any-after-pure-arm"),
            (true, true, true) => w.temp_move(),
            (true, true, false) => w.decline("fan:any-borrowed-pure-arm"),
            (true, false, _) => {}
        }
    }

    /// #2755: `h.f = v` (stmts.rs `lower_field_assign`): the value moves into
    /// the record copy's slot — behind the share guard (`witness_store`), or
    /// as a MOVED temporary's own credit (`m`, #3104). The root var's rebind
    /// is `field_assign_with`'s. A value that spends the var's credit (the
    /// C-132 write-back) leaves the old block to the callee: not recorded.
    pub(crate) fn witness_field_value(&mut self, value: &almide_ir::IrExpr, t: SliceTy, moved: Option<u32>, spends: bool) {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return;
        }
        if spends {
            self.witness_decline("field-assign:spends-var");
            return;
        }
        match moved {
            Some(l) => {
                if let Some(w) = self.witness.as_mut()
                    && !w.move_local(l)
                {
                    w.poison();
                }
            }
            None => self.witness_store(value, t),
        }
    }
}
