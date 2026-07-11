impl LowerCtx {
    /// C1 DEFUNCTIONALIZATION for `list.find` — the EARLY-EXIT scan `xs |> list.find((t) =>
    /// <pred>)` (the gguf/ggml `find_tensor` / `get_metadata_*` shape, whose predicate CAPTURES
    /// the search key, so the lift path can never serve it). A specialized loop over the source
    /// with the FIRST match written into a pre-allocated 0-or-1 OPTION block (the len-as-tag
    /// layout: 1 slot, `len@4` starts 0 = none, overwritten to 1 = some on the hit), then an
    /// immediate loop break.
    ///
    /// SOUNDNESS by REUSE: the option block is ONE fresh owned Alloc (cert `i`) the caller
    /// binds/moves like any self-host Option result; the hit arm's payload acquire is the
    /// per-arm `Dup` (+1, cert `a`) + `Consume` (move-in, `m`) the heap-result-if arms prove —
    /// executed at most once (the break). The predicate lowers via `lower_heap_result_cond`
    /// (scalar / heap-eq / pure-call conds; its transient temps are freed within the cond
    /// frame). Out of subset → `None` (the caller rolls back + WALLs — never invalid wasm).
    /// A RICH record payload routes the option's drop through `optrec:<R>` (the
    /// `materialize_opt_aggregate_some` convention); a matrix-shaped payload has no two-level
    /// option drop yet → defers.
    fn try_lower_defunc_find(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if params.len() != 1 {
            return None;
        }
        let elem_ty = match &xs.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        if !matches!(result_ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1) {
            return None;
        }
        // A matrix-shaped payload (`Option[Matrix]` / `Option[List[List[Float]]]`) would need a
        // two-level option drop this brick does not route — defer (never a row leak).
        if matches!(&elem_ty, Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))
            || crate::lower::is_list_list_str_ty(&Ty::Applied(
                TypeConstructorId::List,
                vec![elem_ty.clone()],
            ))
        {
            return None;
        }
        let elem_heap = is_heap_ty(&elem_ty);
        // Borrow the source list (evaluated once).
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // The result OPTION block: 1 slot, len@4 overwritten to 0 (none) until a hit.
        let one_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_len, value: 1 });
        let opt = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: opt,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: one_len },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![opt] });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let len_addr = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![len_addr, zero] });

        // Loop index + step.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // elem = xs[i] — a borrowed handle (heap) or the i64 value (scalar).
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if elem_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });
        self.value_of.insert(params[0].0, elem);
        if elem_heap {
            self.param_values.insert(elem);
            // An aggregate element's field read (`t.name`) borrows its real slot.
            if matches!(&elem_ty, Ty::Tuple(_)) || self.aggregate_field_tys(&elem_ty).is_some() {
                self.materialized_aggregates.insert(elem);
            }
            self.seed_variant_param(elem, &elem_ty);
        }

        // The predicate (Bool) — scalar / heap-eq / pure-call conds; temps freed in the frame.
        let pred = self.lower_heap_result_cond(body)?;

        // On the hit: write the payload into slot 0, flip len to 1 (some), then break.
        self.ops.push(Op::IfThen { cond: pred, dst: None });
        let slot_addr = self.load_addr(oh, 12);
        if elem_heap {
            let dup = self.fresh_value();
            self.ops.push(Op::Dup { dst: dup, src: elem });
            let ph = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![dup] });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![slot_addr, ph] });
            self.ops.push(Op::Consume { v: dup });
        } else {
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![slot_addr, elem] });
        }
        let one_tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_tag, value: 1 });
        let len_addr2 = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![len_addr2, one_tag] });
        self.ops.push(Op::Else { val: None });
        self.ops.push(Op::EndIf { val: None });

        // Break when found (LoopBreakUnless breaks on 0 → pass 1 - pred), else advance.
        let not_pred = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: not_pred, op: IntOp::Sub, a: one_v, b: pred });
        self.ops.push(Op::LoopBreakUnless { cond: not_pred });
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Drop routing: a RICH record payload frees through the option-wrapper recursion
        // (`optrec:<R>` → `$__drop_<R>` at the wrapper's last ref); a flat heap payload
        // (String / List[scalar] / flat record) keeps the caller's per-slot classification
        // (`heap_elem_lists` — DropListStr over the 0-or-1 slot). The option READS as a
        // materialized 0-or-1 list either way.
        if let Some(rn) = self.record_or_anon_drop_type_name(&elem_ty) {
            self.variant_drop_handles.insert(opt, format!("optrec:{rn}"));
        } else if elem_heap {
            self.heap_elem_lists.insert(opt);
        }
        self.materialized_lists.insert(opt);
        Some(opt)
    }

    /// Lower a `List[String]`-valued LEAF (a list literal, a `+` concat, a named call, or a `??`) to
    /// a FRESH OWNED, TRACKED sublist (in `live_heap_handles` with the recursive `DropListStr` drop) —
    /// a single allocation (`i`), so it is soundly droppable (unlike a merged-if). `None` out of subset.
    fn lower_owned_str_sublist(&mut self, leaf: &IrExpr) -> Option<ValueId> {
        let v = match &leaf.kind {
            // A list LITERAL / `+` CONCAT: the builders mark the right recursive-drop set
            // (`heap_elem_lists` for `List[String]`) but do NOT push to `live_heap_handles` — the
            // uniform registration below does, so `append_owned_sub_to_acc`'s `drop_arm_locals` frees
            // the per-leaf sublist (`id`, not a LEAK).
            IrExprKind::List { .. } => self.try_lower_str_list_literal(leaf)?,
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                self.try_lower_concat_list(leaf)?
            }
            IrExprKind::UnwrapOr { expr, fallback } => {
                // `(value.as_array(val) ?? [])` etc. — the option-unwrap helper returns a fresh owned
                // list (track_result=true registers it in live_heap_handles + the recursive drop set).
                self.try_lower_option_unwrap_or(expr, fallback, true)?
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&leaf.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                // A `List[String]` call result is a fresh owned nested-ownership list — mark its
                // recursive drop (the uniform registration below adds the scope-end free). A
                // matrix-shaped sublist (`List[Matrix]`) needs the nested DropListListStr grain.
                if crate::lower::is_list_list_str_ty(&leaf.ty) {
                    self.list_list_str_lists.insert(dst);
                } else {
                    self.heap_elem_lists.insert(dst);
                }
                dst
            }
            // A `Module` call (`(... ?? []) |> list.flat_map(...)` — the NESTED flat_map leaf): route
            // through the pure-module-call path, which re-enters the defunc HOF lowering and returns a
            // fresh OWNED `List[String]`. Mark it for the recursive drop.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let dst = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &leaf.ty)
                    .ok()?;
                if crate::lower::is_list_list_str_ty(&leaf.ty) {
                    self.list_list_str_lists.insert(dst);
                } else {
                    self.heap_elem_lists.insert(dst);
                }
                dst
            }
            _ => return None,
        };
        // UNIFORM REGISTRATION: every leaf sublist is a FRESH OWNED block — track it in
        // `live_heap_handles` (idempotent) so the per-leaf `drop_arm_locals` after the concat frees it
        // exactly once. WITHOUT this a list-literal/concat leaf (the builders skip the push) would be
        // an `i` with no `d` — a LEAK the proven checker REJECTs.
        if !self.live_heap_handles.contains(&v) {
            self.live_heap_handles.push(v);
        }
        Some(v)
    }

    /// Lower a `some(...)` String PAYLOAD to a FRESH OWNED String handle (rc 1, droppable): a string
    /// literal, a `+` concat, a `${...}` interpolation, a Dup'd tracked/borrowed Var, or a String-
    /// returning named/module call. `None` for any other shape (the HOF then rolls back + WALLs).
    fn lower_owned_str_payload(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            IrExprKind::LitStr { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(value.clone()),
                });
                Some(dst)
            }
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => self.try_lower_concat_str(expr),
            IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
            // A Var (the borrowed element or a let-local) — Dup a fresh owned reference the singleton
            // will own; the original keeps its own reference (dropped at its own scope).
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&expr.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty).ok()
            }
            // `option ?? "fallback"` — a String-returning unwrap (track_result=false: the result is
            // moved into the singleton below, not registered separately).
            IrExprKind::UnwrapOr { expr: inner, fallback } => {
                self.try_lower_option_unwrap_or(inner, fallback, false)
            }
            _ => None,
        }
    }

    /// Materialize a 1-element `List[String]` owning `piece` (an owned String handle MOVED into slot 0)
    /// — a droppable sublist tracked in `live_heap_handles` (recursive `DropListStr`). The SAME block
    /// `materialize_opt_str_some` builds, but pushed for the per-leaf `drop_arm_locals` free.
    fn materialize_str_singleton(&mut self, piece: ValueId) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: obj,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynListStr { len: one },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.heap_elem_lists.insert(obj);
        self.live_heap_handles.push(obj);
        obj
    }

    /// Append an OWNED, DROPPABLE sublist `sub` (created since `arm_mark`) to the loop-carried `acc`
    /// slot: `new = __list_concat_rc(acc, sub)` (borrows both, rc-incs each element into the new list),
    /// then DROP-OLD `acc` + SetLocal the slot to `new` (the proven `i(id)` loop-carried append), then
    /// free `sub` (and any other per-leaf temps) via `drop_arm_locals`. The element refcounts the old
    /// `acc` and `sub` held are released by their drops; the concat already co-acquired them into `new`.
    fn append_owned_sub_to_acc(&mut self, sub: ValueId, acc: ValueId, arm_mark: usize) -> Option<()> {
        let new = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(new),
            name: "__list_concat_rc".to_string(),
            args: vec![CallArg::Handle(acc), CallArg::Handle(sub)],
            result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        // `new` inherits the acc's drop grain (a matrix-elem acc sweeps two levels).
        if self.list_list_str_lists.contains(&acc) {
            self.list_list_str_lists.insert(new);
        } else {
            self.heap_elem_lists.insert(new);
        }
        // DROP-OLD the previous accumulator value, then rebind the slot IN PLACE to `new`. `SetLocal`
        // folds `new`'s `i` into the slot stream (the loop-carried `i(id)m`).
        let drop_acc = self.drop_op_for(acc);
        self.ops.push(drop_acc);
        self.ops.push(Op::SetLocal { local: acc, src: new });
        // Free the per-leaf sublist + any helper temps (the concat borrowed them; their elements are
        // now co-owned by `new`). `drop_arm_locals` emits the recursive `DropListStr` — the balanced
        // `id` for these per-iteration owned temps.
        self.drop_arm_locals(arm_mark);
        Some(())
    }

    /// `base + offset` as a fresh value (the address-arithmetic half of `load_at_offset`,
    /// without the load — used when the loaded address feeds further arithmetic).
    fn load_addr(&mut self, base: ValueId, offset: i64) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        addr
    }
}
