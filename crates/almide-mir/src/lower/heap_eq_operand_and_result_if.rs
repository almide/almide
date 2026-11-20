// The heap `==` operand materialization and the heap-result `if` of
// control_p3_c.rs (moved out of it whole; included from
// control_b.rs after control_p3_c.rs).

impl LowerCtx {
    /// Materialize ONE operand of a heap `==` cond into a block whose handle the eq core reads.
    /// - A tracked heap `Var` is BORROWED: its existing block is returned, NOT added to the cond
    ///   frame (it is owned elsewhere and drops at its own scope — the eq only reads it).
    /// - Any other heap operand (a heap-returning CALL, an Option/Result CTOR, a String literal /
    ///   concat) is materialized into a FRESH OWNED block via `lower_owned_heap_field`, which
    ///   pushes it to `live_heap_handles`; we then register its RECURSIVE drop set
    ///   (`heap_elem_lists` / `value_handles` / …) so the cond-frame `drop_arm_locals` frees it
    ///   AND its owned payload (no leak). The owned temp is freed exactly once (the frame `d`),
    ///   never double-freed (the eq borrows it, never consumes). Returns None for a non-
    ///   materializable shape → the caller walls.
    fn materialize_eq_operand(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &expr.kind {
            // A tracked heap Var (a param or let-local) — borrow its block, no new ownership.
            let v = self.value_for(*id).ok()?;
            // The eq READS the block through the operand type's layout, so a DEFERRED
            // Opaque bind (any heap type — a variant/tuple/record slot read of an empty
            // block is a raw un-bounds-checked load, garbage compared silently) must
            // decline on the strict path: the eq walls and v0 emits the correct bytes
            // (the C-136 wrong-value family, consumer side).
            if crate::lower::strict_values() && self.deferred_opaque_binds.contains(&v) {
                return None;
            }
            // An Option/Result var must additionally be a genuinely MATERIALIZED block
            // (ctor-built, or a param seeded by `seed_variant_param`). A deferred value
            // that escaped the bind set — e.g. an optional-chain result outside the
            // executable subset — has len 0, which the len-as-tag read MISREADS as
            // `none`/`Ok` (`assert_eq(p?.name, some("Alice"))` compared false while
            // native compared true — a silent wrong verdict, the #790 optional-chain
            // row). Decline instead.
            use almide_lang::types::constructor::TypeConstructorId as TC;
            if matches!(ty, Ty::Applied(TC::Option | TC::Result, _))
                && self.value_shapes.get(&v).is_none()
            {
                return None;
            }
            return Some(v);
        }
        // An Option/Result CTOR (`some("")`, `none`, `ok(v)`, `err(m)`) — `try_lower_option_ctor`
        // (which handles BOTH Option and Result ctors) builds the DynListStr/OptSome block +
        // registers its recursive drop set (`materialize_opt_str_some` → `heap_elem_lists` +
        // `materialized_options`), but does NOT push to `live_heap_handles`. Push it so the cond-
        // frame `drop_arm_locals` frees it (and its owned payload) exactly once after the
        // (borrowing) eq.
        if matches!(
            &expr.kind,
            IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
        ) {
            let obj = self.try_lower_option_ctor(expr, ty)?;
            if !self.live_heap_handles.contains(&obj) {
                self.live_heap_handles.push(obj);
            }
            return Some(obj);
        }
        // `rs[i] == ok(v)` — an ELEMENT of a materialized heap-element list (the
        // fan.settle results literal): BORROW the element handle (`$elem_addr` +
        // `LoadHandle` — the list owns it and the eq only reads), the Var-borrow
        // discipline element-precise. Nothing joins the cond frame — no drop, no
        // ownership event; an untracked/deferred container declines inside the
        // borrow and falls through to the materializer below.
        if let IrExprKind::IndexAccess { .. } = &expr.kind {
            if let Some(b) = self.try_lower_heap_field_borrow(expr) {
                return Some(b);
            }
        }
        // Otherwise a heap-returning CALL / literal / concat — materialize a fresh OWNED block
        // (`lower_owned_heap_field` pushes it to `live_heap_handles`). The call path leaves the
        // block FLAT, so register the recursive drop set from the operand TYPE — else an
        // `Option[String]`/`List[String]` temp leaks its inner Strings. Idempotent.
        let obj = self.lower_owned_heap_field(expr)?;
        if self.live_heap_handles.contains(&obj) {
            self.register_owned_heap_eq_drop(obj, ty);
        }
        Some(obj)
    }

    /// Register the recursive drop set for a freshly materialized heap eq-operand block, mirroring
    /// the call-binding tracking in `lower_bind` so the cond-frame teardown frees nested ownership.
    pub(crate) fn register_owned_heap_eq_drop(&mut self, obj: ValueId, ty: &Ty) {
        if let Some(n) = crate::lower::anon_tuple_list_route(ty) {
            // `List[<anon heap tuple>]` (#2520) — the synthesized per-slot sweep.
            self.value_drops.entry(obj).or_default().named_route = Some(n);
        } else if crate::lower::is_list_list_str_ty(ty) {
            self.value_drops.entry(obj).or_default().list_list_str = true;
        } else if crate::lower::is_list_str_str_ty(ty) {
            self.value_drops.entry(obj).or_default().str_str_elems = true;
        } else if crate::lower::is_list_int_str_ty(ty) {
            self.value_drops.entry(obj).or_default().named_route = Some("list_int_str".to_string());
        } else if let Some(n) = self.list_int_variant_drop(ty) {
            // `List[(Int, <rich variant>)]` (list.enumerate_h, #1496).
            self.value_drops.entry(obj).or_default().named_route = Some(n);
        } else if crate::lower::is_lenlist_list_ty(ty) {
            self.value_drops.entry(obj).or_default().named_route = Some("list_lenlist".to_string());
        } else if is_heap_elem_list_ty(ty) {
            // List[heap] / Option[heap] / Result[_, heap] — the DynListStr recursive free.
            self.value_drops.entry(obj).or_default().flat_elems = true;
        }
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(obj);
        }
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_heap_result_cond(cond)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
        // RELEASE PARITY across the arms. An arm may MOVE an OUTER scope-level
        // handle into its result — the effect tail's `err(msg)` moves the error
        // accumulator into the Err block — which removes it from
        // `live_heap_handles` GLOBALLY, though the move runs only on that arm's
        // PATH. The sibling arm must then release it ITSELF: without the
        // compensating Drop the non-moving path LEAKS the handle (one error
        // accumulator per call on the happy path). The flat certificate hid this
        // by counting the moving arm's `m` unconditionally; the branch-grouped
        // cert (`{m|}` — arms disagree) REJECTS it, which is how it was found.
        // With the Drop the arms agree (`{m|d}`) and the leak is gone. Nested
        // heap-result ifs recurse through here, so parity holds level by level.
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        // Each arm starts from the bindings that dominate the `if` (#3267): a
        // rebind made in the then arm (a `mut` param's copy-on-write clone)
        // names a value the else path never defines.
        let bindings = self.value_of.clone();
        let then_obj = self.lower_heap_result_branch(then, result_ty)?;
        let then_bindings = std::mem::replace(&mut self.value_of, bindings.clone());
        let consumed_by_then: Vec<ValueId> =
            outer.iter().copied().filter(|h| !self.live_heap_handles.contains(h)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: then_obj });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let else_obj = self.lower_heap_result_branch(else_, result_ty)?;
        let consumed_by_else: Vec<ValueId> = live_after_then
            .iter()
            .copied()
            .filter(|h| !self.live_heap_handles.contains(h))
            .collect();
        for h in &consumed_by_then {
            if !consumed_by_else.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.push(op); // the ELSE arm releases what THEN moved out
            }
        }
        for h in &consumed_by_else {
            if !consumed_by_then.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.insert(else_marker_at, op); // the THEN arm releases what ELSE moved out
            }
        }
        if then_obj.is_none() && else_obj.is_none() { return None; }
        self.ops.push(Op::EndIf { val: else_obj });
        self.join_arm_bindings(bindings, &then_bindings);
        Some(dst)
    }

    /// After a two-armed `if`: a binding both arms left as it was before the
    /// `if` stays; one an arm rebound has no single value on every path, so it
    /// is unbound and a later read walls instead of naming one arm's value
    /// (#3267). The else arm's bindings are the current ones.
    fn join_arm_bindings(
        &mut self,
        before: std::collections::HashMap<VarId, ValueId>,
        then_bindings: &std::collections::HashMap<VarId, ValueId>,
    ) {
        let else_bindings = std::mem::replace(&mut self.value_of, before);
        self.value_of.retain(|var, v| {
            then_bindings.get(var) == Some(v) && else_bindings.get(var) == Some(v)
        });
    }
}
