// The scalar-loop Result delivery of LowerCtx, include!-spliced back from mod_p3.rs.
impl LowerCtx {
    /// Extracted from [`Self::lower_stmt_assign_scalar_loop`] (codopsy r2, #852): the
    /// `ResultOk`/`ResultErr` arm's repr dispatch — decides which Result-block builder
    /// (cap-tag str vs len-as-tag Ok/Err) materializes the delivered base value.
    /// `value` is the reassigned constructor; `expr` is its payload.
    ///
    /// Repr dispatch: a HEAP-Ok Result (`Result[String,_]` — base64
    /// decode_chunks) is the cap-tag block `materialize_result_str`
    /// builds; a SCALAR-Ok Result (`Result[Int,String]` — the
    /// early-return `res` accumulator) is the LEN-AS-TAG family, so
    /// routing it through the str builder emitted a scalar payload
    /// into a handle slot — invalid wasm (i32/i64 mismatch) that
    /// ESCAPED the render wall (probe-confirmed). Build len-tag:
    /// Ok → `materialize_result_ok` (len 0, scalar @12); Err →
    /// `materialize_opt_str_some` (len 1, owned String @12 — the
    /// same physical block `try_lower_result_err_variant_ctor`
    /// uses; the slot's bind-time tracking already frees slot-0
    /// on the Err path via DropListStr).
    fn lower_scalar_loop_result_delivery(
        &mut self,
        value: &IrExpr,
        expr: &IrExpr,
    ) -> Option<ValueId> {
        let is_err = matches!(&value.kind, IrExprKind::ResultErr { .. });
        if Self::is_heap_ok_result(&value.ty) {
            match (self.lower_result_str_piece(expr), repr_of(&value.ty)) {
                (Some(piece), Ok(repr)) => {
                    Some(self.materialize_result_str(piece, repr, is_err, false))
                }
                _ => None,
            }
        } else if is_err {
            match (self.lower_result_str_piece(expr), repr_of(&value.ty)) {
                (Some(piece), Ok(repr)) => {
                    Some(self.materialize_opt_str_some(piece, repr))
                }
                _ => None,
            }
        } else {
            match (self.lower_scalar_value(expr), repr_of(&value.ty)) {
                (Some(payload), Ok(repr)) => {
                    Some(self.materialize_result_ok(payload, repr))
                }
                _ => None,
            }
        }
    }

    /// [`Self::lower_stmt_assign_fallback`]'s in-place rebind of a heap var's own
    /// slot inside a frame that really runs: `Some(Ok)` once rebound, `None`
    /// (every op rolled back) where the var has no owned slot of its own or the
    /// value has no fresh-owned producer.
    fn try_frame_slot_rebind(&mut self, var: VarId, value: &IrExpr) -> Option<Result<(), LowerError>> {
        let local = *self.value_of.get(&var)?;
        if !self.live_heap_handles.contains(&local) || self.param_values.contains(&local) {
            return None;
        }
        let (ops, handles) = (self.ops.len(), self.live_heap_handles.len());
        match self.lower_scalar_loop_heap_reassign(var, value) {
            Ok(()) => Some(Ok(())),
            Err(_) => {
                self.ops.truncate(ops);
                self.live_heap_handles.truncate(handles);
                None
            }
        }
    }
}
