// Typed aggregate field borrows; included by tail.rs.
impl LowerCtx {
    pub(crate) fn try_lower_heap_field_borrow(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        if !is_heap_ty(&expr.ty) {
            return None;
        }
        // A HEAP-element list index `xs[i]` (`xs: List[String]`) — LoadHandle the element's OWNED
        // handle at the bounds-checked `$elem_addr(list, i)` as a BORROW (the list still owns it,
        // freed by its DropListStr; the read is not a second owner → `param_values`). Without this
        // a heap `xs[i]` fell through to the container-grain `Dup` (the WHOLE list), which a String
        // consumer then read as a String = the list HEADER bytes (the `$ ` garbage). Gated to a
        // tracked/materialized list var so `$elem_addr` reads a real populated block (else defer).
        if let IrExprKind::IndexAccess { .. } = &expr.kind {
            return self.try_lower_heap_list_elem_borrow(expr);
        }
        let (container, offset) = match &expr.kind {
            IrExprKind::Member { object, field } => {
                (object, self.aggregate_field_offset_any(&object.ty, field.as_str())?)
            }
            IrExprKind::TupleIndex { object, index } => {
                (object, self.aggregate_index_offset_any(&object.ty, *index)?)
            }
            _ => return None,
        };
        // The container must be MATERIALIZED with the uniform slot layout — a tracked heap VAR
        // (its block built by `try_lower_*_construct`, a param-bound aggregate), OR a NESTED
        // aggregate field whose inner block is itself materialized. A DEREFERENCING heap-field
        // read of a DEFERRED `Alloc{Opaque}` aggregate (garbage slots) would load a junk handle
        // and TRAP at `rc_dec`, so the resolver returns `None` for it (the caller falls back to
        // the safe container-grain Dup). The Var case gates on `materialized_aggregates`; the
        // nested case recurses through this same borrow (gated at each level).
        let h = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => {
                let src = self.value_or_global(*id).ok()?;
                // The container must be a REAL block with the uniform slot layout: a locally
                // materialized aggregate, OR a BORROWED aggregate handle owned elsewhere
                // (`param_values` — a function param, a destructure/match-bound payload handle).
                // A `param_values` aggregate handle ALWAYS points at a real block (a deferred
                // `Init::Opaque` aggregate is owned-and-tracked in `live_heap_handles`, never
                // borrowed), so dereferencing its slot is sound — EXACTLY as the sibling
                // `try_lower_{tuple,record}_destructure` already trust `param_values` for the
                // identical `LoadHandle(container + offset)`. This closes the asymmetry that left a
                // match-bound record payload's String field (`match o { some(r) => r.k }`) falling to
                // the container-grain `Dup` (which read the record HEADER as a String = garbage).
                if !self.materialized_aggregates.contains(&src) && !self.param_values.contains(&src) {
                    return None;
                }
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
                h
            }
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
                if is_heap_ty(&container.ty) =>
            {
                let inner = self.try_lower_heap_field_borrow(container)?;
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![inner] });
                h
            }
            // A RECORD-literal container (`{ blob: …, tag: "t" }.blob` — a pure
            // call-initialized global after `inline_pure_call_globals` substituted its
            // init, #2739): build it into an ANF temp through the `let tmp = { … }` path
            // (tracked, recursive scope-end drop), then borrow from that block like a
            // materialized local's.
            IrExprKind::Record { .. } | IrExprKind::Call { .. }
            | IrExprKind::UnwrapOr { .. } | IrExprKind::If { .. }
            | IrExprKind::Match { .. } | IrExprKind::Block { .. }
                if is_heap_ty(&container.ty) && (matches!(container.kind, IrExprKind::Record { .. })
                    || crate::lower::owned_tco_slots()) => {
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &container.ty, container).ok()?;
                let src = self.value_for(tmp).ok()?;
                if !self.materialized_aggregates.contains(&src) {
                    return None;
                }
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![src] });
                h
            }
            _ => return None,
        };
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(dst), args: vec![addr] });
        // BORROWED: the container owns the field; the read is not a second owner.
        self.param_values.insert(dst);
        Some(dst)
    }

    /// Read-only predicate mirroring [`Self::try_lower_heap_field_borrow`]'s container gate: does a
    /// field/index extraction over `container` resolve to a MATERIALIZED/param heap-aggregate block
    /// (so a borrow would succeed)? Used by the list-literal builder's `all_lowerable` pre-check to
    /// admit a `Member`/`TupleIndex` element ONLY when the loop's borrow `?` will succeed — keeping the
    /// pre-check and the build loop in lockstep (no partial-ops leak on a mid-build decline).
    pub(crate) fn heap_field_container_tracked(&self, container: &IrExpr) -> bool {
        match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self
                .value_for(*id)
                .map(|src| {
                    self.materialized_aggregates.contains(&src) || self.param_values.contains(&src)
                })
                .unwrap_or(false),
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
                if is_heap_ty(&container.ty) =>
            {
                self.heap_field_container_tracked(object)
            }
            _ => false,
        }
    }

}
