impl LowerCtx {
    /// Try to lower `for i in start..end { body }` over a SCALAR Int index as a REAL loop
    /// that EXECUTES every step — desugaring the range to the same while machinery
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd` + `SetLocal`). The index is its own stable
    /// local initialized to `start` and incremented by 1 each iteration; `end` is snapshot
    /// ONCE before the loop (v0 builds the range once). Restricted to the runnable subset:
    /// a LITERAL `start` (so the index local is a fresh, distinct `ConstInt` — safe to
    /// mutate, never aliasing a caller value), a scalar-lowerable `end`, an Int loop var
    /// (no tuple), no `break`/`continue`, and a heap-reassign-free body (the
    /// `scalar_loop_depth` rule errors otherwise). Returns false (rolled back) when out of
    /// subset; `lower_for_in` then falls back to its sound model-one-iteration form.
    pub(crate) fn try_lower_scalar_for_range(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        let IrExprKind::Range { start, end, inclusive } = &iterable.kind else {
            return false;
        };
        if var_tuple.is_some()
            || body_breaks_or_continues(body)
            || matches!(find_var_ty(body, var), Some(t) if !matches!(t, Ty::Int))
            || !matches!(start.kind, IrExprKind::LitInt { .. })
        {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Snapshot `end` once; init the index local `i = start` (a fresh ConstInt — a
        // distinct, mutable local, never aliasing a caller value). `one` for the step.
        let end_v = match self.lower_scalar_value(end) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                return false;
            }
        };
        if self.lower_bind(var, &Ty::Int, start).is_err() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        let Some(&i_v) = self.value_of.get(&var) else {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        };
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        // The bound test, re-read each iteration: `i < end` (exclusive) / `i <= end` (incl).
        let cond_v = self.fresh_value();
        let cmp = if *inclusive { IntOp::Le } else { IntOp::Lt };
        self.ops.push(Op::IntBinOp { dst: cond_v, op: cmp, a: i_v, b: end_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        // The implicit step `i = i + 1`, then the back-edge.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }

    /// EXECUTE `for x in xs { … }` over a `List[T]` as a real loop (vs the model-one-iteration
    /// form): borrow the list handle once, walk an internal index `i` 0..len via the loop markers,
    /// bind element `i` to the loop var `x` each iteration, run the body.
    ///
    /// TWO element shapes, BOTH borrowing the list (read-only; the list keeps owning its elements):
    /// - a SCALAR element (`List[Int/Float/Bool]`, i64 slots) — `Load { width: 8 }` the slot and
    ///   `SetLocal` the loop var (a stable mutable i64 local, a COPY, no ownership);
    /// - a HEAP element (`List[String]` / nested-ownership DynListStr, i32-handle slots) — the loop
    ///   var is the BORROWED element handle, `LoadHandle`d fresh each iteration into `value_of[var]`
    ///   and recorded in `param_values` so it is NOT a second owner (the list's recursive drop frees
    ///   the element; the loop var must not free it — no double-free). The body reads the element via
    ///   string/list ops; a body that MOVES the element out (stores it elsewhere) is not in this
    ///   subset (the borrow stays read-only), so such a body rolls back.
    ///
    /// SOUND by reuse of the for-range / while machinery: the body is per-iteration-balanced
    /// (`drop_arm_locals`), the markers no-op in the cert (it verifies ONE balanced iteration), the
    /// `i < len` guard runs the body the REAL number of times (0 for an empty list — closing the
    /// model-one-iteration bug that ran a heap-element body ONCE on a garbage handle). GATED to a
    /// `List[scalar]` / heap-element list, a matching loop-var type, no tuple/break/continue.
    pub(crate) fn try_lower_scalar_for_list(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        use crate::PrimKind;
        // The element type: a scalar `List[Int/Float/Bool]` (i64 slot) OR a heap-element list
        // (`List[String]`, i32-handle slot). A Map / non-list iterable defers.
        let elem_ty = match &iterable.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return false,
        };
        let elem_heap = is_heap_ty(&elem_ty);
        // A heap-AGGREGATE element (tuple/record) is bound below as the slot's BORROWED block handle
        // (`LoadHandle` + registered in `materialized_aggregates`), so a direct FIELD/INDEX projection
        // (`for p in ps { p.0 }` / `for r in rs { r.x }`) projects off the ELEMENT block — the same
        // per-element borrow map/filter give a `List[record]`/`List[Value]` lambda param. A `let (x, y)
        // = p` destructure (tuple PATTERN) or passing `p` whole already worked; both now share the
        // element-precise borrow.
        let elem_is_aggregate = elem_heap && self.aggregate_field_tys(&elem_ty).is_some();
        // The element SHAPE (scalar vs heap) comes from the iterable's element type, so the loop var
        // is bound correctly even when it is UNUSED in the body (an `for _ in xs`, or a loop kept for
        // its effect count) — `find_var_ty` returns None then, which must NOT fall to the model-one-
        // iteration form (that ran the body ONCE; an empty list must run it ZERO times). When the var
        // IS used, its body-declared type must agree with the element shape (a defensive consistency
        // gate against a mis-typed body).
        let var_ty = find_var_ty(body, var);
        if let Some(vt) = &var_ty {
            if is_heap_ty(vt) != elem_heap {
                return false;
            }
        }
        if var_tuple.is_some() || body_breaks_or_continues(body) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Borrow the list (evaluated once); a Var is borrowed, a fresh literal is materialized
        // (owned, dropped at the outer scope — it stays in live_heap_handles). A heap-element
        // list LITERAL (`for s in ["x", "y"]`) needs its elements actually stored, so route it
        // through `try_lower_str_list_literal` (the filled owned list) rather than the generic
        // `lower_call_args` Alloc path (which would leave an empty/opaque block → zero iterations).
        let str_list_literal =
            elem_heap && matches!(&iterable.kind, IrExprKind::List { elements } if !elements.is_empty());
        let list_v = if str_list_literal {
            match self.try_lower_str_list_literal(iterable) {
                Some(v) => {
                    self.live_heap_handles.push(v);
                    v
                }
                None => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        } else {
            match self.lower_call_args(std::slice::from_ref(iterable)) {
                Ok(args) => match args.into_iter().next() {
                    Some(CallArg::Handle(v)) => v,
                    _ => {
                        self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                        return false;
                    }
                },
                Err(_) => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        // The SCALAR loop var is a stable mutable i64 local, `SetLocal` to element[i] each iteration.
        // (A HEAP loop var is bound fresh per iteration below — no stable local: a borrowed i32
        // handle re-`LoadHandle`d inside the loop.)
        let x_v = if elem_heap {
            None
        } else {
            let x = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: x, value: 0 });
            self.value_of.insert(var, x);
            Some(x)
        };

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });
        // The element-slot address `h + 12 + i*8`.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: i8_v });
        if let Some(x_v) = x_v {
            // Scalar element: x = load64(slot) — a COPY into the stable mutable local.
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![addr] });
            self.ops.push(Op::SetLocal { local: x_v, src: elem });
        } else {
            // Heap element: x = the BORROWED i32 handle at the slot (LoadHandle, Ptr repr), bound
            // fresh each iteration. Recorded in `param_values` — the list still OWNS the element
            // (its recursive DropListStr frees it), so the loop var is NOT a second owner and is
            // NOT added to the per-iteration drop set (no double-free).
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(elem), args: vec![addr] });
            self.value_of.insert(var, elem);
            self.param_values.insert(elem);
            // A heap-AGGREGATE element (tuple/record): register the borrowed block handle as a
            // materialized aggregate so a `p.0`/`r.x` field projection and a `let (x, y) = p`
            // destructure read the ELEMENT's slots (not the container) — the same per-element borrow
            // map/filter give an aggregate lambda param. The list still OWNS the element (its
            // recursive drop frees it), so this is a BORROW (already in `param_values`), not a second
            // owner — no double-free.
            if elem_is_aggregate {
                self.materialized_aggregates.insert(elem);
            }
        }

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }
}
