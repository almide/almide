// ── part of binds.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    /// Phase 5 of [`Self::lift_lambda`]: materialize the CLOSURE BLOCK itself — the
    /// all-scalar-capture `ListLit` fast path, else the prim alloc + per-capture
    /// Dup/Store/Consume co-own. Verbatim.
    fn materialize_closure_block(
        &mut self,
        name: String,
        layout: &CaptureLayout,
        cap_vals: Vec<ValueId>,
    ) -> ValueId {
        // Materialize the CLOSURE BLOCK: a DynList of 2 + k slots — slot 0 the funcref
        // table index, slot 1 the SELF-DESCRIBING drop header (n_heap | n_nested_heap<<16
        // | n_closure<<32 — three 16-bit counts, what lets the uniform `$__drop_closure`
        // free any closure block at any drop site without lowering-time mask knowledge),
        // then the captures (closure, flat heap, nested heap, scalar).
        // Rung-5 closures slab: an ALL-SCALAR-capture env block is a plain slot list
        // ([fnidx, drop-header=0, scalars…]), so the TARGET-NEUTRAL `Op::ListLit`
        // builds it on both legs — same cert `i`, same block bytes on wasm, a
        // `Vec<i64>` on native. Heap/closure captures keep the prim path below
        // (their Dup/Consume co-own dance needs the address stores).
        if layout.handle_slots() == 0 {
            let fr = self.fresh_value();
            self.ops.push(Op::FuncRef { dst: fr, name });
            let hdr = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: hdr, value: 0 });
            let mut elems: Vec<ValueId> = Vec::with_capacity(2 + cap_vals.len());
            elems.push(fr);
            elems.push(hdr);
            elems.extend(cap_vals.iter().copied());
            let blk = self.fresh_value();
            self.ops.push(Op::ListLit { dst: blk, elems });
            // EXACT tracking mirror of the prim path below.
            self.live_heap_handles.push(blk);
            self.closure_values.insert(blk);
            return blk;
        }
        // RICH captures (#1547 shapes 2/3): wrap each captured list in a fresh
        // 2-slot `[tag@12][list-handle@20]` block FIRST — the env slot then holds
        // the wrapper, and `$__drop_closure`'s rich arm hands it to the generated
        // `__drop_env_rich` (tag → the element type's recursive list free). The
        // wrapper co-owns the list (`Dup` + move-in, the original var's scope-end
        // drop untouched) and is itself MOVED into the env below (no Dup — it
        // exists solely for this block).
        let mut cap_vals = cap_vals;
        let mut rich_wrappers: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
        for (i, (cap_var, cap_ty)) in layout.captures.iter().enumerate() {
            if !layout.is_rich(i) {
                continue;
            }
            // A ROUTED cell capture tags `cell:<map drop name>` (the dispatcher's
            // cell arms free the co-owned CELL — wrapper @20 holds the cell, whose
            // @12 holds the map); a plain rich list capture tags its element name.
            let tag_name = if self.cell_of.contains_key(cap_var) {
                self.map_named_value_drop(cap_ty)
                    .map(|n| format!("cell:{n}"))
                    .expect("Routed cell admitted only via map_named_value_drop")
            } else {
                self.rich_capture_elem_name(cap_ty)
                    .or_else(|| self.record_capture_name(cap_ty).or_else(|| self.variant_capture_name(cap_ty)).map(|n| format!("one:{n}")))
                    .expect("rich class admitted only via rich_capture_elem_name / record_capture_name / variant_capture_name")
            };
            let tag_val = crate::lower::rich_env_tag(&tag_name);
            let two = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: two, value: 2 });
            let w = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst: w,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: Init::DynList { len: two },
            });
            if self.cell_of.contains_key(cap_var) {
                self.aggregate_blocks.insert(w, crate::lambda_sigs::cell_wrapper_key(cap_ty));
            } else {
                self.aggregate_blocks.insert(w, crate::render_native_rec::block_key(&Ty::Tuple(vec![Ty::Int, cap_ty.clone()].into())));
            }
            let wh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(wh), args: vec![w] });
            let tv = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: tv, value: tag_val });
            let o12 = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: o12, value: 12 });
            let s0 = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: s0, op: IntOp::Add, a: wh, b: o12 });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s0, tv] });
            let owned = self.fresh_value();
            self.ops.push(Op::Dup { dst: owned, src: cap_vals[i] });
            let lh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(lh), args: vec![owned] });
            let o20 = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: o20, value: 20 });
            let s1 = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: s1, op: IntOp::Add, a: wh, b: o20 });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s1, lh] });
            self.ops.push(Op::Consume { v: owned });
            self.live_heap_handles.retain(|x| *x != owned);
            cap_vals[i] = w;
            rich_wrappers.insert(w);
        }
        let len_c = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len_c, value: (2 + cap_vals.len()) as i64 });
        let blk = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: blk,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len: len_c },
        });
        let fr = self.fresh_value();
        self.ops.push(Op::FuncRef { dst: fr, name });
        let hdr = self.fresh_value();
        self.ops.push(Op::ConstInt {
            dst: hdr,
            value: (layout.n_heap as i64)
                | ((layout.n_nested_heap as i64) << 16)
                | ((layout.n_closure as i64) << 32)
                | ((layout.n_cellmap as i64) << 48)
                | ((layout.n_rich as i64) << 56),
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![blk] });
        for (i, v) in [fr, hdr].into_iter().chain(cap_vals.iter().copied()).enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(i) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // A closure/heap/nested-heap capture: the closure CO-OWNS it — `Dup` a fresh
            // reference (CowSafety makes the share value-semantics-safe: any later in-place
            // mutation clones-on-shared), store its handle, `Consume` the fresh ref into
            // the block (cert `a` + `m`; the original var's scope-end drop is untouched).
            // The fnidx/header/scalar slots store the raw value.
            let cap_index = i as i64 - 2; // captures start at slot 2
            if cap_index >= 0 && (cap_index as usize) < layout.handle_slots() {
                // A RICH wrapper is MOVED in (built above solely for this env slot —
                // a Dup would strand one reference); every other handle capture keeps
                // the Dup co-own.
                let owned = if rich_wrappers.contains(&v) {
                    v
                } else {
                    let owned = self.fresh_value();
                    self.ops.push(Op::Dup { dst: owned, src: v });
                    owned
                };
                let handle = self.fresh_value();
                self.ops
                    .push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![owned] });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, handle],
                });
                self.ops.push(Op::Consume { v: owned });
                self.live_heap_handles.retain(|x| *x != owned);
            } else {
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, v],
                });
            }
        }
        // A fresh owned heap value: dropped at scope end unless a consumer moves it out
        // (a tail return removes it from the live set). `closure_values` routes its drop
        // to the recursive `$__drop_closure` (`drop_op_for`).
        self.live_heap_handles.push(blk);
        self.closure_values.insert(blk);
        blk
    }
}
