// the scalar value lowering entry points of calls_p3.rs, include!-spliced back at
// module level (a code move under the 800-line ceiling the codopsy gate holds this
// crate to).

impl LowerCtx {
    /// Lower a SCALAR (Int/Bool/Float) value expression to a `ValueId` holding its REAL
    /// value, or `None` (the caller then DEFERS to `Const`). SELF-ROLLBACK contract: on a
    /// `None` return this restores BOTH `self.ops` AND `self.live_heap_handles` to their
    /// entry length, so the function leaves NO net side effect when it fails — a caller may
    /// roll back with an `ops`-only truncate (the historic discipline) and still be correct
    /// even though a sub-lowering (a scalar CALL OPERAND, `5 + string.len("abc")`) may
    /// MATERIALIZE a fresh heap argument temp (an `Alloc` registered for a scope-end drop).
    /// On SUCCESS, any such temp stays tracked (it is a genuine value to free at scope end),
    /// exactly as a direct `let _ = string.len("abc")` bind tracks it. The actual lowering
    /// is [`Self::lower_scalar_value_inner`].
    pub(crate) fn lower_scalar_value(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => Some(v),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Lower a scalar operand of an EAGER `UnOp` (`not e`) or logical `And`/`Or`, FREEING any
    /// transient heap temp the operand materializes WITHIN a local frame. The canonical case is
    /// `c == "'"` (→ `string.eq(c, "'")`): the `"'"` literal is a fresh owned String that is dead
    /// the instant the `Bool` is computed, so it is `Alloc`'d (cert `i`) and `Drop`'d (cert `d`)
    /// LOCALLY here — the operand is internally balanced and registers NO temp in the enclosing
    /// frame. This is SOUND precisely because `and`/`or`/`not` are EAGER in v0 (both operands /
    /// the operand always evaluate, NO short-circuit), so the `Drop` always runs on the same path
    /// as the `Alloc`; the scalar `Bool` result survives the frame teardown (it is not a heap
    /// handle). Returns `None` (fully rolled back) if the operand is not scalar-lowerable. (Before,
    /// a heap-materializing operand was GATED OUT to `None` → the caller fell back to a silent
    /// `Const 0` / a `WALL` — the `not (c == "'" or c == "\"")` miscompile.)
    fn lower_scalar_operand(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let frame = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => {
                // Free any transient temp the operand allocated (e.g. a string-eq literal),
                // keeping the operand internally `i…d`-balanced — the scalar `v` is not among them.
                // A handle the operand handed to an OUTER variable is not a temp: it stays
                // tracked in the enclosing scope, which frees it with that variable.
                let kept = self.frame_handles_of_outer_assigns(expr, frame);
                if !kept.is_empty() {
                    let tail = self.live_heap_handles.split_off(frame.min(self.live_heap_handles.len()));
                    self.live_heap_handles.extend(tail.into_iter().filter(|h| !kept.contains(h)));
                }
                self.drop_arm_locals(frame);
                self.live_heap_handles.extend(kept);
                Some(v)
            }
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(frame);
                None
            }
        }
    }

    /// The handles tracked since `frame` that now ARE the value of a variable declared
    /// OUTSIDE the operand block: the move-mode write-back in an operand
    /// (`not reparse(doc, …)` → `{ let t = reparse(doc, …); doc = t.1; t.0 }`) gives
    /// `doc` a fresh reference that outlives the operand, so the operand's teardown
    /// must not free it (it did: `doc` was read after its block was dropped).
    fn frame_handles_of_outer_assigns(&self, expr: &IrExpr, frame: usize) -> Vec<ValueId> {
        let IrExprKind::Block { stmts, .. } = &expr.kind else { return Vec::new() };
        let declared: Vec<almide_ir::VarId> = stmts
            .iter()
            .filter_map(|s| match &s.kind {
                IrStmtKind::Bind { var, .. } => Some(*var),
                _ => None,
            })
            .collect();
        let frame_handles = &self.live_heap_handles[frame.min(self.live_heap_handles.len())..];
        let mut kept = Vec::new();
        for s in stmts {
            let IrStmtKind::Assign { var, .. } = &s.kind else { continue };
            if declared.contains(var) {
                continue;
            }
            if let Some(&h) = self.value_of.get(var) {
                if frame_handles.contains(&h) && !kept.contains(&h) {
                    kept.push(h);
                }
            }
        }
        kept
    }
}
