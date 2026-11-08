// the defunctionalized HOF loop's per-iteration store and finish, include!-spliced
// back from defunc_hof_inner.rs at module level (a code move under the 800-line
// ceiling the codopsy gate holds this crate to).

impl LowerCtx {
    /// The per-iteration STORE: write this iteration's result into the loop state —
    /// map: `result[i] = body_v`; filter: conditionally keep the element and bump the
    /// cursor; fold: rebind the accumulator slot.
    fn store_defunc_iteration(
        &mut self,
        func: &str,
        st: &DefuncLoopSt,
        body_v: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        match func {
            "map" => {
                // result[i] = body_v.
                let rh = st.result_h.expect("result_h is Some here: the \"map\" arm is only reached when func == \"map\", which seeds result_h in setup");
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: st.i8_v });
                if st.has_result_elem {
                    // body_v is an OWNED heap handle (i32) already Consumed by lower_heap_result_arm
                    // (moved out of the iteration scope). Extend it to i64 (`PrimKind::Handle`,
                    // exactly the str-list-literal store) then store64 into the slot — the result list
                    // now owns it (its recursive DropListStr frees it at scope end).
                    let eh = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![body_v] });
                    self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, eh] });
                } else {
                    self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, body_v] });
                }
                Some(())
            }
            "filter" => {
                // if body_v (Bool) then { result[cursor] = elem; cursor += 1 }.
                let rh = st.result_h.expect("result_h is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds result_h in setup");
                let cur = st.cursor.expect("cursor is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds cursor in setup");
                self.ops.push(Op::IfThen { cond: body_v, dst: None });
                // then-arm: store elem at result[cursor*8], bump cursor.
                let c8 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: c8, op: IntOp::Mul, a: cur, b: st.eight });
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: c8 });
                // A HEAP filter keeps the source ELEMENT (a BORROWED handle, `param_values`): CLONE it
                // (Dup, cert `a` = a new owned ref) and MOVE it into the output list (Consume, cert `m`).
                // The `a..m` is LOCALLY balanced — both in THIS then-arm, the else-arm does nothing — so
                // the existing flat certificate accepts it WITHOUT a loop-carried conditional slot (the
                // output list is alloc'd once, not a SetLocal-rebound slot; per kept element a fresh
                // Dup'd object is acquired and immediately moved into the list, whose recursive
                // DropListStr/DropListValue frees it). A SCALAR filter stores the i64 value directly (no
                // ownership). OwnershipFilter.v's CondLoop proves the more general loop-carried form; this
                // locally-balanced shape needs only the base checker.
                let stored = if st.has_result_elem {
                    let cloned = self.fresh_value();
                    self.ops.push(Op::Dup { dst: cloned, src: st.elem });
                    self.ops.push(Op::Consume { v: cloned });
                    let eh = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![cloned] });
                    eh
                } else {
                    st.elem
                };
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, stored] });
                let cnext = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: cnext, op: IntOp::Add, a: cur, b: st.one_v });
                self.ops.push(Op::SetLocal { local: cur, src: cnext });
                self.ops.push(Op::Else { val: None });
                self.ops.push(Op::EndIf { val: None });
                Some(())
            }
            "fold" => {
                // acc = body_v. A HEAP acc DROPS the old slot value first (the loop-carried `i(id)m`
                // append-accumulator pattern: each transient String reclaimed), then moves the new one
                // in. A scalar acc just rebinds (no handle to free).
                if st.heap_fold_acc {
                    let drop_op = self.drop_op_for(st.acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"));
                    self.ops.push(drop_op);
                }
                self.ops.push(Op::SetLocal { local: st.acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"), src: body_v });
                Some(())
            }
            _ => None,
        }
    }

    /// The HOF's VALUE after the loop closes: the fold accumulator, the map result
    /// list, or the filter result list with its `len` patched to the write-cursor.
    fn finish_defunc_hof(&mut self, func: &str, st: &DefuncLoopSt) -> Option<ValueId> {
        use crate::PrimKind;
        match func {
            // A HEAP acc's final value is an OWNED String returned to the caller, which registers it
            // for the outer scope-end drop (the same as the map/filter result list — C1 does NOT push
            // it itself, or it would be double-dropped).
            "fold" => Some(st.acc_local.expect("acc_local is Some here: this arm only runs when func == \"fold\"")),
            "map" => Some(st.result_list.expect("result_list is Some here: this arm only runs when func == \"map\"")),
            "filter" => {
                // Patch the result list's `len` field (offset 4) to the write-cursor: the
                // visible length is the count of kept elements (cap stays len(xs), unused
                // tail slots are harmless — a `${list}` Display reads `len`, an `xs[i]`
                // bounds-checks against `len`). `store32` at result_h + 4.
                let rh = st.result_h.expect("result_h is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds result_h in setup");
                let cur = st.cursor.expect("cursor is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds cursor in setup");
                let four = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: four, value: 4 });
                let lenaddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lenaddr, cur] });
                Some(st.result_list.expect("result_list is Some here: this arm only runs when func == \"filter\", which shares result_list with map"))
            }
            _ => None,
        }
    }
}
