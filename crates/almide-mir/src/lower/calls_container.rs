// Aggregate container evaluation shared by field and tuple reads.
impl LowerCtx {
    /// Resolve an aggregate CONTAINER expression to the i64 BYTE-ADDRESS of its block (the base
    /// for a `base + slot_offset` field load). A `Var` bound to a tracked heap aggregate (or a
    /// param-bound aggregate) is `Prim::Handle`'d directly. A NESTED aggregate field (`o.p` in
    /// `o.p.x`) is borrowed via `try_lower_heap_field_borrow` (the loaded inner-block handle) then
    /// `Prim::Handle`'d — so field access composes to arbitrary depth over materialized blocks.
    /// `None` for a non-resolvable container (`f().x`, a non-materialized var) → the caller defers.
    pub(crate) fn resolve_aggregate_container_handle(&mut self, container: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        let block = self.resolve_aggregate_container_block(container)?;
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![block] });
        Some(h)
    }

    /// The container's BLOCK value (pre-`Handle`) — the form the target-neutral
    /// list/record ops take (`Op::ListGetScalar` resolves its own address).
    pub(crate) fn resolve_aggregate_container_block(&mut self, container: &IrExpr) -> Option<ValueId> {
        let block = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_or_global(*id).ok()?,
            // A nested aggregate field — borrow its loaded inner-block handle. Gated on the
            // OUTER container being materialized (inside `try_lower_heap_field_borrow`), so a
            // garbage slot is never dereferenced.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if is_heap_ty(&container.ty) =>
            {
                self.try_lower_heap_field_borrow(container)?
            }
            // A list-ELEMENT aggregate (`line.items[ii].idx` — the chained scalar read off
            // an indexed record): borrow the element block via the same bounds-checked
            // `$elem_addr` LoadHandle the for-in element borrow uses (gated on a tracked/
            // field-borrowable list container at each level).
            IrExprKind::IndexAccess { .. } if is_heap_ty(&container.ty) => {
                self.try_lower_heap_field_borrow(container)?
            }
            // A CALL-result container (`mk_paren().name` — the paren-ctor scalar field read):
            // ANF-materialize the call to a synthetic temp via the SAME `lower_bind` path a
            // `let tmp = mk_paren()` takes (tracked, recursive scope-end drop, read shapes
            // seeded), then resolve the temp — the exact mirror of `lower_heap_extraction`'s
            // Call arm on the scalar-field side.
            // A RECORD-literal container (`{ blob: …, tag: "t" }.tag` — what a reference
            // to a pure call-initialized global becomes once `inline_pure_call_globals`
            // substitutes its init, #2739) takes the same ANF temp: the literal is built
            // once, read, and dropped at scope end.
            IrExprKind::Call { .. } | IrExprKind::Record { .. } if is_heap_ty(&container.ty) => {
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &container.ty, container).ok()?;
                self.value_for(tmp).ok()?
            }
            // A selected aggregate is evaluated once into owned storage before
            // extracting a field. Keep branch evaluation in lower_bind so an
            // untaken fallback cannot run or expose an uninitialized payload.
            IrExprKind::UnwrapOr { .. } | IrExprKind::If { .. }
            | IrExprKind::Match { .. } | IrExprKind::Block { .. }
                if crate::lower::owned_tco_slots() && is_heap_ty(&container.ty) => {
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &container.ty, container).ok()?;
                self.value_for(tmp).ok()?
            }
            _ => return None,
        };
        Some(block)
    }

}
