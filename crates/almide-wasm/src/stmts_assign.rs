// ── stmts.rs, assignment and field-assignment lowering ──
//
// include!-spliced into `stmts.rs` at module level (the 800-line file
// discipline). The imports are stmts.rs's own.

impl Emitter<'_> {
    /// `Assign` lowering — the share/dec discipline (RC-3/RC-5) plus the
    /// growing-accumulator window, split from `lower_stmt` for the
    /// complexity budget.
    fn lower_assign(&mut self, var: &almide_ir::VarId, value: &IrExpr) -> Result<(), EmitError> {
                let moved = self.take_moved_temp(value);
                if self.routine(|r: &bool| *r, |em| em.try_str_append_assign(var, value))? {
                    return Ok(());
                }
                if self.routine(|r: &bool| *r, |em| em.try_list_append_assign(var, value))? {
                    return Ok(());
                }
                if self.routine(|r: &bool| *r, |em| em.try_map_set_assign(var, value))? {
                    return Ok(());
                }
                let (local, declared) = match self.locals.get(var) {
                    Some(&(idx, d)) => (Some(idx), d),
                    None => match self.globals.get(&(self.var_space, *var)) {
                        Some(&(gidx, d)) => (None, {
                            let _ = gidx;
                            d
                        }),
                        None => return unsup("assign:unmapped"),
                    },
                };
                // #1688 once refused a droppable PARAM reassigned under an
                // if/match arm ("one path releases the caller's block, the
                // other keeps it"). Under the credit discipline the local
                // holds exactly ONE credit on every path — the assign
                // releases the old occupant and makes the local an owner,
                // the epilogue releases the local once whichever arm ran —
                // and the exit validator (E083) checks it; the refusal is
                // retired (stage 2c-ii: records made the mut_port cell hit it).
                // #3406: `var` dies at a consuming rhs over it (dying_move.rs).
                self.note_dying(value, Some(*var), true);
                // #4208: a tail `map.remove(var, k)` of a tombstone var holes
                self.with_tomb_tail(*var, value, |em| em.lower(value, Some(declared)))?;
                self.note_dying(value, Some(*var), false);
                // RC-5: same share discipline as Bind — except a MOVED
                // temp (#3104), whose one credit becomes the var's.
                if self.rc_droppable(declared) && !self.rc_owned_result(value) && moved.is_none() {
                    self.rc_inc_top();
                }
                // RC-3: same ownership settlement as Bind — here for locals
                // (a global's is at the store below), never through a cell, and
                // NEVER when the rhs SPENDS the assigned var's own credit
                // (`assign_rhs_spends_var`): then the callee already released
                // or reallocated the old block, and a dec here double-frees
                // (mut_heap_param exit-1'd on exactly this). Every other rhs
                // leaves the old occupant this local's to release. Aliasing
                // rhs shapes (`xs = if c then xs else ys`) stay safe by order:
                // the RC-5 inc above runs before this dec, so a same-block
                // result nets to zero.
                let rhs_spends_var = self.assign_rhs_spends_var(value, *var);
                if let Some(idx) = local
                    && !self.cells.contains(var)
                    && self.rc_droppable(declared)
                    && !rhs_spends_var
                {
                    let dec = self.dec_fn_of(declared);
                    self.f.instructions().local_get(idx).call(dec);
                    self.rc_own(idx, declared);
                }
                // The witness (#2757): a droppable local's occupant changes
                // here — or a global's / a cell's, an outer holder the frame
                // borrows (#2755, `witness_holder`), settled the same way below.
                if self.rc_droppable(declared)
                    && let Some(idx) = self.witness_holder(*var, local.is_none())
                {
                    match moved {
                        Some(src) => self.witness_transfer(idx, !rhs_spends_var, src),
                        None => self.witness_assign(idx, !rhs_spends_var, rhs_spends_var, value),
                    }
                }
                // #2010: a C-319 cell's occupant is released as it is
                // replaced (the cell holds exactly one credit on it) — the
                // same settlement, one load deeper.
                if let Some(idx) = local
                    && self.cells.contains(var)
                    && self.rc_droppable(declared)
                    && !rhs_spends_var
                {
                    let dec = self.dec_fn_of(declared);
                    self.f.instructions().local_get(idx);
                    self.load_ty_slot(declared, 0);
                    self.f.instructions().call(dec);
                }
                match local {
                    Some(idx) => self.emit_store_var(*var, idx, declared)?,
                    None => {
                        let gidx = self.globals[&(self.var_space, *var)].0;
                        // #2992: a top-let global holds ONE credit on its
                        // occupant for the program's life (its initializer
                        // takes it, func.rs), so replacing the occupant
                        // releases it — the local settlement above, read
                        // through the global. Without it every `g = …` in a
                        // loop kept the previous block alive.
                        if self.rc_droppable(declared) && !rhs_spends_var {
                            let dec = self.dec_fn_of(declared);
                            self.f.instructions().global_get(gidx).call(dec);
                        }
                        self.f.instructions().global_set(gidx);
                    }
                }
                self.empty_moved_temp(moved);
                Ok(())
    }
}

impl Emitter<'_> {
    /// `p.field = v` on a record var: copy-on-write write-back — fresh
    /// block, one slot replaced, rebound. Split from `lower_stmt` for the
    /// complexity budget; the core is `field_assign_with` (list_mut.rs),
    /// which the record-field forms of the mut list/map ops share (#2411).
    pub(crate) fn lower_field_assign(
        &mut self,
        target: &almide_ir::VarId,
        path: &[almide_base::intern::Sym],
        value: &IrExpr,
    ) -> Result<(), EmitError> {
        let moved = self.take_moved_temp(value);
        if moved.is_none() && self.routine(|r: &bool| *r, |em| em.try_field_append_assign(target, path, value))? {
            return Ok(());
        }
        let spends_var = self.assign_rhs_spends_var(value, *target);
        self.field_assign_with(target, path, spends_var, |s, fty| {
            s.lower(value, Some(fty))?;
            if moved.is_none() {
                s.rc_share_guard(value, fty);
            }
            s.witness_field_value(value, fty, moved, spends_var);
            Ok(())
        })?;
        self.empty_moved_temp(moved);
        Ok(())
    }
}
