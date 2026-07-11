impl LowerCtx {
    pub(crate) fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), LowerError> {
        // First try to EXECUTE it as a real scalar-state loop; on any out-of-subset
        // feature this rolls back cleanly and we reach the model-one-iteration form below.
        if self.try_lower_scalar_while(cond, body) {
            return Ok(());
        }
        // The fallback below runs the body straight-line ONCE (the model-one-iteration
        // form). A `break`/`continue` (no early-exit branch) and a HEAP ACCUMULATOR
        // reassignment (deferred → the accumulation is dropped) BOTH make that one
        // iteration produce the wrong answer — WALL them rather than silently miscompile.
        // (Walling BEFORE lowering the body avoids emitting partial ops; the executable
        // `try_lower_scalar_while` already declined both shapes and rolled back.)
        self.wall_break_over_heap_frame(body, "while", self.live_heap_handles.len())?;
        if body_reassigns_heap(body) {
            return Err(LowerError::Unsupported(
                "while body with a heap-accumulator reassignment cannot be faithfully lowered \
                 (the model-one-iteration fallback defers the reassignment, dropping the \
                 accumulation) not in this brick"
                    .into(),
            ));
        }
        self.record_elided_calls(cond);
        let mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Post-lowering loop-body admission for `break`/`continue` reaching the
    /// MODEL-ONE-ITERATION fallback (the executable `try_lower_scalar_*` paths already
    /// decline a break/continue body and roll back, so this is only hit when the loop
    /// linearizes to one modeled iteration). That fallback runs the body straight-line
    /// ONCE with NO loop and NO early-exit branch, so it CANNOT honor an early exit: the
    /// break/continue is silently dropped and the loop produces the wrong answer (e.g.
    /// `while i<100 { if i==7 then break; i=i+1 }; print(i)` → v0 `7`, the one-iteration
    /// form `1`). WALL it — a break/continue is faithfully executed only by the real-loop
    /// markers (`try_lower_scalar_while`/`_for_*`), which do not yet cover early exits.
    /// (This SUBSUMES the prior heap-frame leak wall: a heap-frame early exit would also
    /// skip a per-iteration Drop, but the selection bug walls every break/continue first.)
    pub(crate) fn wall_break_over_heap_frame(
        &self,
        body: &[IrStmt],
        what: &str,
        _mark: usize,
    ) -> Result<(), LowerError> {
        if body_breaks_or_continues(body) {
            return Err(LowerError::Unsupported(format!(
                "{what} body with break/continue cannot be faithfully lowered (the model-one-iteration fallback runs the body once with no early-exit branch, losing the break/continue) not in this brick"
            )));
        }
        Ok(())
    }
}
