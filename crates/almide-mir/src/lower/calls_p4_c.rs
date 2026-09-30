impl LowerCtx {

    /// The name → `PrimKind` table for the prim calls lowering synthesizes (see
    /// [`Self::lower_prim_call`]); any other `prim.*` name is stdlib-internal (#3025).
    fn prim_kind_for_name(&self, func: &str) -> Result<crate::PrimKind, LowerError> {
        if matches!(func, "budget_enter" | "budget_exhausted" | "budget_exit" | "budget_spend") {
            crate::charge_probe::note_budget_used();
            return Ok(match func {
                "budget_enter" => crate::PrimKind::BudgetEnter,
                "budget_exhausted" => crate::PrimKind::BudgetExhausted,
                "budget_spend" => crate::PrimKind::BudgetSpend,
                _ => crate::PrimKind::BudgetExit,
            });
        }
        if matches!(func, "timeout_enter" | "timeout_exit" | "timeout_hit") {
            crate::charge_probe::note_timeout_used();
            return Ok(match func {
                "timeout_enter" => crate::PrimKind::TimeoutEnter,
                "timeout_hit" => crate::PrimKind::TimeoutHit,
                _ => crate::PrimKind::TimeoutExit,
            });
        }
        match func {
            "handle" => Ok(crate::PrimKind::Handle),
            "die" => Ok(crate::PrimKind::Die),
            _ => Err(LowerError::Unsupported(format!(
                "prim.{func} is internal to the standard library (#3025)"
            ))),
        }
    }

    /// One argument of [`Self::emit_prim_call`].
    fn lower_prim_call_one_arg(&mut self, func: &str, a: &IrExpr, kind: crate::PrimKind) -> Result<ValueId, LowerError> {
        use crate::PrimKind;
        // A STRING-LITERAL argument to `prim.handle` — the frontend's single-use
        // let-inliner pushes `let tbl = "…"; prim.handle(tbl)` into
        // `prim.handle("…")` (the generated case-mapping tables). Materialize
        // the literal block exactly as its let-bound form would (owned Alloc,
        // scope-end drop) and hand the prim its handle — the scalar-tail
        // deferred-Const fallback was silently returning 0 as the address.
        if matches!(kind, PrimKind::Handle) {
            if let IrExprKind::LitStr { value } = &a.kind {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: repr_of(&a.ty)?,
                    init: crate::Init::Str(value.clone()),
                });
                self.live_heap_handles.push(dst);
                return Ok(dst);
            }
            // A COMPUTED String argument (`prim.die(prim.handle("assertion failed: "
            // + msg))` — the 2-arg assert's computed-message die): materialize the
            // concat/interp chain to an owned block (scope-tracked, dropped at the
            // arm/scope end like the literal above) and hand the prim its handle.
            // Without this the whole assert's unit-if rolled back and the wall
            // (misleadingly) named the CONDITION.
            if matches!(
                &a.kind,
                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. }
                    | IrExprKind::StringInterp { .. }
            ) {
                let obj = match &a.kind {
                    IrExprKind::BinOp { .. } => self.try_lower_concat_str(a),
                    IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
                    _ => unreachable!(),
                };
                if let Some(obj) = obj {
                    self.live_heap_handles.push(obj);
                    return Ok(obj);
                }
            }
        }
        self.lower_scalar_value(a).ok_or_else(|| {
            LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
        })
    }

    fn emit_prim_call(&mut self, func: &str, args: &[IrExpr], kind: crate::PrimKind) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        let mut lowered = Vec::with_capacity(args.len());
        for a in args {
            lowered.push(self.lower_prim_call_one_arg(func, a, kind)?);
        }
        // `prim.die` is the one Unit op `prim_kind_for_name` yields.
        let dst = if matches!(kind, PrimKind::Die) { None } else { Some(self.fresh_value()) };
        self.ops.push(Op::Prim { kind, dst, args: lowered });
        Ok(dst)
    }

    /// Extracted from `Self::materialized_call_arg` (codopsy7 max-depth sweep): the
    /// mutually-exclusive drop-route selection for a fresh heap call-argument temp,
    /// verbatim — the original `if/else if` chain rewritten as independent
    /// `if COND { ...; return; }` guards (same order, same first-match-wins semantics,
    /// pure control-flow rewrite). Was nested one level deeper than the sibling bind-site
    /// routers (`seed_call_named_heap_drop_route` et al.) because it lives inside the
    /// caller's `if repr.is_heap() { .. }` — extracting it to its own `&mut self` method
    /// resets the naive depth counter to 1 for every arm.
    fn seed_call_arg_heap_drop_route(&mut self, dst: ValueId, ty: &Ty) {
        // A `value.as_array(v) ?? []` arg temp (the materialized `??` operand) is a
        // Result[List[Value],String] that OWNS its inner list — its drop must free the list AND
        // its element Values RECURSIVELY (`DropResultListValue`); the flat `heap_elem_lists`
        // fallback would only rc_dec the inner-list handle, LEAKING the element Values (a loop
        // OOMs). Checked BEFORE is_heap_elem_list_ty, which also matches this Result type.
        // (A Result[Value,String]'s Ok Value is CO-OWNED — value.get Dup's the object's slot, which
        // keeps its ref — so the flat rc_dec drop is correct there; a recursive free would
        // double-free the still-referenced slot. So only the list case is reclassified here.)
        if crate::lower::is_res_fs_ty(ty) {
            // `Result[(Float, String), String]` arg temp (result.zip_fs fed to
            // unwrap_or_else) — the same tag-aware route the bind sites seed.
            self.value_drops.entry(dst).or_default().named_route = Some("res_fs".to_string());
            return;
        }
        if crate::lower::is_res_lenlist_str_ty(ty) {
            // Heap-Ok `Result[List[<one-level-exact>], String]` arg temp — the same
            // tag-aware `$__drop_res_lsl` route the bind sites seed.
            self.value_drops.entry(dst).or_default().named_route = Some("res_lsl".to_string());
            return;
        }
        if crate::lower::is_result_listval_ty(ty) {
            self.value_drops.entry(dst).or_default().value_result_list = true;
            return;
        }
        if let Some(df) = self.variant_pair_result_drop_fn(ty) {
            // `Result[(V1, V2), String]` arg temp (#1547 shape 1 — a transition
            // result fed straight to a consumer) — the same recursive
            // `$__drop_vp_<A>_<B>` wrapper route the bind sites seed.
            self.value_drops.entry(dst).or_default().named_route = Some(format!("resrec:{df}"));
            return;
        }
        if crate::lower::is_list_list_str_ty(ty) {
            self.value_drops.entry(dst).or_default().list_list_str = true;
            return;
        }
        if let Some(n) = crate::lower::anon_tuple_list_route(ty) {
            // `List[<anon heap tuple>]` (#2520) — the synthesized per-slot sweep; the
            // flat heap_elem_lists DropListStr would leak every tuple's heap slots.
            self.value_drops.entry(dst).or_default().named_route = Some(n);
            return;
        }
        if crate::lower::is_list_str_str_ty(ty) {
            // `List[(String,String)]` (map.entries) arg temp — DropListStrStr frees each tuple's
            // two Strings; the flat heap_elem_lists fallback would leak them.
            self.value_drops.entry(dst).or_default().str_str_elems = true;
            return;
        }
        if crate::lower::is_lenlist_list_ty(ty) {
            self.value_drops.entry(dst).or_default().named_route = Some("list_lenlist".to_string());
            return;
        }
        if crate::lower::is_map_fn_ty(ty) {
            // `Map[String, <Fn>]` arg temp — `$__drop_map_mclo` frees each value via
            // `__drop_closure` (a flat sweep would leak every captured env slot).
            self.value_drops.entry(dst).or_default().named_route = Some("map_mclo".to_string());
            return;
        }
        self.seed_call_arg_map_drop_route(dst, ty);
    }

    /// The Map- and record-shaped arg-temp drop routes of
    /// [`Self::seed_call_arg_heap_drop_route`]. Split at the named-value seam so
    /// neither half outgrows a readable guard ladder; the ORDER across the two
    /// halves is unchanged (this one runs only after every guard above declined).
    fn seed_call_arg_map_drop_route(&mut self, dst: ValueId, ty: &Ty) {
        if let Some(hname) = self.map_named_value_drop(ty) {
            self.value_drops.entry(dst).or_default().named_route = Some(hname);
            return;
        }
        if crate::lower::is_map_msv_ty(ty) {
            // `Map[String, Map[String, String]]` arg temp (the inline nested-map literal
            // fed straight to `map.get_or` — map_fold_heap_acc's r7): `$__drop_map_msv`
            // sweeps each last-ref inner map; the flat fallback leaked the whole nested
            // map per iteration (loop OOM).
            self.value_drops.entry(dst).or_default().named_route = Some("map_msv".to_string());
            return;
        }
        if crate::lower::is_map_mlo_ty(ty) {
            // `Map[String, List[Option[Int]]]` arg temp — `$__drop_map_mlo` (the
            // bind-site route, mirrored; the flat fallback would leak the value lists).
            self.value_drops.entry(dst).or_default().named_route = Some("map_mlo".to_string());
            return;
        }
        if crate::lower::is_map_hval_ty(ty) {
            // `Map[String, <flat heap value>]` arg temp — `$__drop_map_hval` sweeps ALL
            // 2n slots. The flat fallback's len-slot sweep reads len@4 = n (the ENTRY
            // count) over a block holding 2n slots, so it freed only the KEY half and
            // leaked every VALUE block. The site that hit it: `__hvf_at`'s recursion
            // passes `map_set_hval(...)` as a nested call ARGUMENT, so every
            // INTERMEDIATE map of a `["k0": v0, "k1": v1, …]` literal dropped here —
            // (n−1) leaked value blocks per literal, output-invisible; found by the
            // #1530 heap-cap harness on its first live run (the msv arm above is the
            // same fix for the nested-map family; hval was simply missing).
            self.value_drops.entry(dst).or_default().named_route = Some("map_hval".to_string());
            return;
        }
        if crate::lower::is_map_ivh_ty(ty) {
            // `Map[Int, String]` arg temp — `$__drop_map_ivh` (the bind-site route,
            // mirrored): same 2n-slot layout, same flat-fallback key-half hole.
            self.value_drops.entry(dst).or_default().named_route = Some("map_ivh".to_string());
            return;
        }
        if let Some(rname) = match ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }  {
            // A `List[<recursive-drop record>]` arg temp — `$__drop_list_<R>` (the
            // bind-site route, mirrored; the flat fallback leaked each element's
            // String fields — the krec-unique residue).
            self.value_drops.entry(dst).or_default().named_route = Some(format!("list_{rname}"));
            return;
        }
        if matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
        {
            // `Map[String, <scalar>]` arg temp — the key-slot sweep (split layout, @4 = n),
            // mirroring the bind-site fix; the flat fallback leaked every key copy.
            self.value_drops.entry(dst).or_default().flat_elems = true;
            return;
        }
        if crate::lower::is_heap_elem_list_ty(ty) {
            self.value_drops.entry(dst).or_default().flat_elems = true;
        }
    }

    /// Register a freshly-materialized call-result temp used as a call argument: a
    /// HEAP temp is BORROWED into the call (`Handle`) and added to the scope-end
    /// drop set (it is owned by THIS scope, not moved out, so it is released after
    /// the call returns); a scalar temp is passed by value. A NESTED-OWNERSHIP temp
    /// (a `List[String]` from `set.from_list(string.split(…))`, etc.) is ALSO recorded
    /// in `heap_elem_lists` so its scope-end drop is the recursive `DropListStr` that
    /// frees the owned element Strings — a flat `Drop` would free only the block and
    /// LEAK the elements (per-iteration in a loop → OOM). Cert is unchanged: one `i`
    /// (alloc) + one `d` (drop) for the temp; DropListStr vs Drop is the runtime
    /// realization of that same single `d`.
    pub(crate) fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr, ty: &Ty) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            self.seed_call_arg_heap_drop_route(dst, ty);
            // A `Value` call-argument temp (`f(value.array([…]))`, `f(value.str(s))`) drops via the
            // runtime-tag-dispatched `Op::DropValue` (recursive — an Array frees its element Values, a
            // Str its String), NOT a flat `Op::Drop` (which would leak the nested payload). Without
            // this a tag-5 Array / tag-4 Str passed as an argument leaks at the call-site scope end.
            if crate::lower::is_value_ty(ty) {
                self.value_handles.insert(dst);
            }
            // A RECORD/TUPLE call-argument temp (`f(mk(x))` — a fresh record passed by handle) drops at
            // the call-site scope end. Without a mask it falls to a flat `Op::Drop` (rc_dec the record
            // block only), LEAKING every heap field (the `f(mk(x))`-in-a-loop OOM). Seed its heap-slot
            // `record_masks` (the masked drop frees the leaf fields) and, when a field is a
            // Map/List[heap]/record/Value, route to the recursive `$__drop_<R>` via variant_drop_handles.
            if let Some((_, tys)) = self.aggregate_field_tys(ty) {
                let heap_slots: Vec<usize> =
                    (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                self.record_masks.insert(dst, heap_slots);
                if let Some(name) = self.record_drop_type_name(ty) {
                    self.value_drops.entry(dst).or_default().named_route = Some(name);
                }
            }
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }
}
