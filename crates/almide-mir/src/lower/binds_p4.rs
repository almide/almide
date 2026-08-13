impl LowerCtx {
    /// BORROW the heap handle at `container_handle + offset` (`LoadHandle` — the container keeps its
    /// own reference) then `Dup` it to a fresh OWNED reference (cert `a`) the caller's aggregate
    /// owns. Tracked in `live_heap_handles` so the caller's `Consume` (move-in) balances it. The
    /// borrow+Dup pair is the SAME machinery `try_lower_spread_record_construct` uses for a copied
    /// heap field — no double-free (two distinct refs, two distinct drops).
    pub(crate) fn dup_borrowed_slot(&mut self, container_handle: ValueId, offset: u32) -> ValueId {
        use crate::{IntOp, PrimKind};
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: container_handle, b: off });
        let borrowed = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(borrowed), args: vec![addr] });
        let owned = self.fresh_value();
        self.ops.push(Op::Dup { dst: owned, src: borrowed });
        self.live_heap_handles.push(owned);
        owned
    }

    /// Shared block-builder for a scalar tuple/list: lower each element to a scalar value,
    /// then emit ONE target-neutral [`Op::ListLit`] (rung 4 — the wasm render expands it to
    /// the exact `DynList`-alloc + per-slot-store sequence this built inline before; the
    /// native leg maps it to `vec![…]`). Element ownership-free (scalars), flat drop, the
    /// identical single-`i` certificate the replaced `Alloc` carried.
    pub(crate) fn try_lower_scalar_list_slots(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None;
        }
        // Lower each field's scalar value first (before the literal op, so a field expr
        // that itself allocates doesn't interleave with the block build).
        let vals: Vec<ValueId> = elements
            .iter()
            .map(|e| self.lower_scalar_value(e))
            .collect::<Option<Vec<_>>>()?;
        let dst = self.fresh_value();
        self.ops.push(Op::ListLit { dst, elems: vals });
        // A REAL, POPULATED scalar list block — admit a direct `xs[i]` bounds-checked load.
        self.materialized_lists.insert(dst);
        Some(dst)
    }

    /// Extract each field of a tuple `subject` (a heap block) into its bound var via a precise
    /// per-slot `Prim` read: a SCALAR field is a value COPY (`Load width 8`), a HEAP field is the
    /// BORROWED slot handle (`LoadHandle`, recorded in `param_values` — the tuple still OWNS the
    /// element, freed by its masked scope-end drop, so the bound var is NOT a second owner). A heap
    /// field is admitted ONLY when the subject is a TRACKED owning aggregate (`materialized_
    /// aggregates`, with a `record_masks` heap-slot mask) or a borrowed PARAM/element handle
    /// (`param_values` — the caller owns it): in both cases reading the slot is a borrow with a
    /// guaranteed single owner, never a leak/double-free. Otherwise (an untracked heap subject —
    /// no mask to free the borrowed inner block) it returns `false` and the caller falls back to
    /// the container-grain `bind_pattern` (still memory-safe, just imprecise) so we never emit a
    /// dangling borrow. Returns `false` for any non-`Bind`/`Wildcard` sub-pattern (a nested tuple
    /// pattern in ONE statement is deferred — sz4 splits it into two statements, which works).
    /// The nested-Option/Result-payload seeding for
    /// [`Self::try_lower_tuple_destructure`]'s per-slot heap bind (`let (a, b) = (some(7),
    /// some(8))` — a tuple slot that is itself Option/Result): tracks `v`'s READ-shape so a
    /// later `match a { .. }` BRANCHES instead of LINEARIZING. NOT
    /// [`Self::seed_nested_option_result_bind_payload`] (control_p2.rs) — that sibling ALSO
    /// checks `is_lenlist_list_ty` first (routing to a `list_lenlist` `variant_drop_handles`
    /// entry before the flat `heap_elem_lists` fallback), which this call site's original
    /// inline chain never did; reusing it here would add new behavior. Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn seed_tuple_slot_option_result_bind(&mut self, v: ValueId, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        if matches!(ty, Ty::Applied(TypeConstructorId::Option, _)) {
            self.materialized_options.insert(v);
            if crate::lower::is_heap_elem_list_ty(ty) {
                self.heap_elem_lists.insert(v);
            }
            return;
        }
        if crate::lower::is_result_ty(ty) {
            self.materialized_results.insert(v);
            if crate::lower::is_heap_elem_list_ty(ty) {
                self.heap_elem_lists.insert(v);
            }
        }
    }

    pub(crate) fn try_lower_tuple_destructure(
        &mut self,
        pats: &[IrPattern],
        subject: ValueId,
        subject_ty: Option<&Ty>,
    ) -> bool {
        use crate::{IntOp, PrimKind};
        // A pattern component's recorded ty can be an UNSUBSTITUTED TypeVar after mono (the
        // generic zip_with's `let (a, b) = p` — patterns lag value-side substitution): treating
        // it as heap loaded an Int slot with i32 (LoadHandle) while the call site passed it
        // scalar-raw — INVALID WASM (the zip_with__Int_String_String i64/i32 mismatch,
        // 2026-07-17). Resolve each component from the SUBJECT's own Tuple ty; a component
        // unresolved on BOTH sides DECLINES the precise destructure (the container-grain
        // fallback is imprecise but never emits a wrong-width load).
        let resolve = |i: usize, pty: &Ty| -> Option<Ty> {
            if !matches!(pty, Ty::TypeVar(_) | Ty::Unknown) {
                return Some(pty.clone());
            }
            if let Some(Ty::Tuple(ts)) = subject_ty {
                if ts.len() == pats.len() && !matches!(ts[i], Ty::TypeVar(_) | Ty::Unknown) {
                    return Some(ts[i].clone());
                }
            }
            None
        };
        // Does the subject OWN its heap slots (a tracked masked aggregate) OR is it a borrow whose
        // owner is elsewhere (a param / a borrowed element handle)? Either way a per-slot HEAP read
        // is a sound borrow. An untracked owned heap subject would leak the borrowed inner block, so
        // a heap field over it must defer to the container-grain alias.
        let heap_borrow_ok =
            self.materialized_aggregates.contains(&subject) || self.param_values.contains(&subject);
        let mut eff_tys: Vec<Option<Ty>> = vec![None; pats.len()];
        for (i, p) in pats.iter().enumerate() {
            match p {
                IrPattern::Bind { ty, .. } => {
                    let Some(eff) = resolve(i, ty) else {
                        return false;
                    };
                    if is_heap_ty(&eff) && !heap_borrow_ok {
                        return false;
                    }
                    eff_tys[i] = Some(eff);
                }
                IrPattern::Wildcard => {}
                _ => return false,
            }
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subject] });
        for (i, p) in pats.iter().enumerate() {
            if let IrPattern::Bind { var, ty: _ } = p {
                let ty = eff_tys[i].as_ref().expect("validated above");
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                let v = self.fresh_value();
                if is_heap_ty(ty) {
                    // BORROW the slot's owned handle (an i32 Ptr). The tuple keeps ownership (its
                    // masked drop frees it), so the bound var joins `param_values` (not a second
                    // owner, not in the scope-end drop set). A nested tuple/record handle bound this
                    // way is itself a tracked aggregate iff the subject's mask owns it — record it so
                    // a FURTHER `(ix, iy) = inner` destructure of it can also borrow its heap slots.
                    self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v), args: vec![addr] });
                    self.param_values.insert(v);
                    if matches!(ty, Ty::Tuple(_)) || self.aggregate_field_tys(ty).is_some() {
                        self.materialized_aggregates.insert(v);
                    }
                    // A tuple slot that is itself an Option/Result (`let (a, b) = (some(7), some(8))`):
                    // track the borrowed slot handle so a later `match a { some(n) => … }` BRANCHES
                    // (reads its tag) instead of LINEARIZING (running every arm + a garbage 0). Same
                    // materialized-Option read-shape Batch 2 gave the payload-bound / field-Option match
                    // subjects — a BORROW of the tuple's owned slot, no new ownership.
                    self.seed_tuple_slot_option_result_bind(v, ty);
                    // A CLOSURE slot (`let (g, _) = pair` where slot 0 is a Fn — the
                    // first-class storage class): the borrowed handle IS a closure
                    // block — admit it to the dispatch set so a later `g()` lowers
                    // to `CallIndirect` instead of walling on an unknown callee.
                    if matches!(ty, Ty::Fn { .. }) {
                        self.closure_values.insert(v);
                    }
                } else {
                    self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
                }
                self.value_of.insert(*var, v);
            }
        }
        true
    }

    /// Per-field record destructure `let { x, y } = p` — the record sibling of
    /// `try_lower_tuple_destructure`. A record block is `[rc][len][cap][f0@12, f1@20, …]` with fields
    /// at the SAME uniform `slot_offset(idx)` as tuple elements (idx = the field's declaration
    /// position in `rec_ty`). Each pattern field is loaded from its OWN slot: a SCALAR field is a
    /// value COPY (`Load{width:8}`); a HEAP field is the slot's BORROWED owned handle (`LoadHandle` =
    /// i32 Ptr — the record keeps ownership via its masked drop, so the bound var joins `param_values`,
    /// not the scope-end drop set). WITHOUT this, `bind_pattern` aliased the WHOLE record pointer for
    /// each field (`Op::Dup`/`Const 0`) → `i64.add` on two record pointers (invalid wat) / String
    /// fields = NUL bytes. `rec_ty` supplies the declaration order (field name → index → offset).
    /// Returns false (caller falls back to container-grain) on an unresolvable field / a heap field
    /// over a non-borrowable subject.
    fn try_lower_record_destructure(
        &mut self,
        fields: &[almide_ir::IrFieldPattern],
        rec_ty: &Ty,
        subject: ValueId,
    ) -> bool {
        use crate::{IntOp, PrimKind};
        let Some((names, tys)) = self.aggregate_field_tys(rec_ty) else {
            return false;
        };
        // A heap field is a BORROW — sound only when the subject owns its slots (a tracked masked
        // aggregate) or is itself a borrow (param / borrowed handle). Mirror the tuple gate.
        let heap_borrow_ok =
            self.materialized_aggregates.contains(&subject) || self.param_values.contains(&subject);
        // Pre-resolve every pattern field to (var, ty, slot index); bail if any is shorthand
        // (no bound var), unknown, or a heap field without a borrowable subject.
        let mut binds: Vec<(VarId, Ty, usize)> = Vec::with_capacity(fields.len());
        for f in fields {
            let var = match &f.pattern {
                Some(IrPattern::Bind { var, .. }) => *var,
                _ => return false, // shorthand / nested / non-bind field — defer
            };
            let Some(idx) = names.iter().position(|n| n.as_str() == f.name.as_str()) else {
                return false;
            };
            let fty = tys[idx].clone();
            if is_heap_ty(&fty) && !heap_borrow_ok {
                return false;
            }
            binds.push((var, fty, idx));
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subject] });
        for (var, fty, idx) in binds {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (idx as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let v = self.fresh_value();
            if is_heap_ty(&fty) {
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v), args: vec![addr] });
                self.param_values.insert(v);
                if matches!(fty, Ty::Tuple(_)) || self.aggregate_field_tys(&fty).is_some() {
                    self.materialized_aggregates.insert(v);
                }
            } else {
                self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
            }
            self.value_of.insert(var, v);
        }
        true
    }

    /// Introduce the variables a destructuring `pattern` binds, CONTAINER-GRAIN: a
    /// HEAP payload/field/element aliases the WHOLE `subject` (`Op::Dup`), a SCALAR one
    /// is a `Const`. Aliasing the container keeps it (and thus the bound value within
    /// it) alive for the binding's lifetime — a conservative lifetime WIDENING that
    /// can never shorten a lifetime, so never a use-after-free; and it reuses the
    /// proven `a`/`Op::Dup` event, so the Coq checker and the `#a == #Dup` backing gate
    /// are UNCHANGED. HONEST SCOPE (value-content, NOT safety): a bound var denotes "a
    /// reference to the SUBJECT", not "the payload's value" — payload/field-PRECISE
    /// aliasing needs the layout brick (offsets + per-field heap-ness) and is deferred,
    /// exactly like `Init::Opaque` content. WALLED: a `RecordPattern` shorthand field
    /// (`{ name }` — no bound `VarId` to thread) and a heap binding over a non-heap
    /// subject (the container has no handle to `Dup`).
    pub(crate) fn bind_pattern(
        &mut self,
        pattern: &IrPattern,
        subject: Option<ValueId>,
    ) -> Result<(), LowerError> {
        match pattern {
            IrPattern::Wildcard | IrPattern::None | IrPattern::Literal { .. } => Ok(()),
            IrPattern::Bind { var, ty } => self.bind_leaf(*var, ty, subject),
            // Carrier patterns bind at the container grain — the payload is the
            // same subject.
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                self.bind_pattern(inner, subject)
            }
            // A tuple-pattern match arm (`match t { (a, b) => … }`) must read each component from
            // its tuple SLOT (base+12+i*8), NOT alias the whole tuple container-grain (which left
            // `a`/`b` reading the tuple pointer / an uninitialized 0). Route through the same
            // layout-aware per-slot loader `let (a, b) = t` uses, when the subject is a tracked
            // materialized aggregate (its slots are real). Falls back to the container-grain
            // recursion below only when there is no per-slot subject (an untracked/None subject).
            IrPattern::Tuple { elements } if self.per_slot_subject(subject).is_some() => {
                let subj = self.per_slot_subject(subject).expect("guarded above");
                if self.try_lower_tuple_destructure(elements, subj, None) {
                    return Ok(());
                }
                self.bind_each(elements, subject)
            }
            IrPattern::Constructor { args: elements, .. }
            | IrPattern::Tuple { elements }
            | IrPattern::List { elements } => self.bind_each(elements, subject),
            IrPattern::RecordPattern { fields, .. } => self.bind_record_fields(fields, subject),
        }
    }

    /// The subject when it has REAL per-slot storage (a tracked materialized
    /// aggregate or a param), which is what the slot-wise tuple loader needs.
    fn per_slot_subject(&self, subject: Option<ValueId>) -> Option<ValueId> {
        subject.filter(|s| {
            self.materialized_aggregates.contains(s) || self.param_values.contains(s)
        })
    }

    /// A `Bind` leaf: a heap binding ALIASES the container (`Dup`, tracked live),
    /// a scalar one gets a deferred `Const`.
    fn bind_leaf(
        &mut self,
        var: almide_ir::VarId,
        ty: &Ty,
        subject: Option<ValueId>,
    ) -> Result<(), LowerError> {
        let dst = self.fresh_value();
        if is_heap_ty(ty) {
            let src = subject.ok_or_else(|| {
                LowerError::Unsupported(
                    "heap pattern binding over a non-heap subject (no container to alias) not in this brick".into(),
                )
            })?;
            self.ops.push(Op::Dup { dst, src });
            self.live_heap_handles.push(dst);
        } else {
            if crate::lower::strict_values() {
                return Err(crate::lower::strict_const_wall("destructure component"));
            }
            self.ops.push(Op::Const { dst });
        }
        self.value_of.insert(var, dst);
        Ok(())
    }

    /// Bind every sub-pattern at the container grain.
    fn bind_each(
        &mut self,
        patterns: &[IrPattern],
        subject: Option<ValueId>,
    ) -> Result<(), LowerError> {
        for p in patterns {
            self.bind_pattern(p, subject)?;
        }
        Ok(())
    }

    /// A record pattern's fields. A shorthand field (`{ name }`) has no bound
    /// `VarId` to thread, so it walls.
    fn bind_record_fields(
        &mut self,
        fields: &[almide_ir::IrFieldPattern],
        subject: Option<ValueId>,
    ) -> Result<(), LowerError> {
        for f in fields {
            let Some(p) = &f.pattern else {
                return Err(LowerError::Unsupported(
                    "record pattern shorthand field (no bound VarId) not in this brick".into(),
                ));
            };
            self.bind_pattern(p, subject)?;
        }
        Ok(())
    }

    /// If `value` is an Option CONSTRUCTOR in the executable subset — `Some(scalar)`
    /// or `None` — lower it to a MATERIALIZED 0-or-1-element-list block and TRACK the
    /// resulting `dst` as a materialized Option, so a later variant `match` over it may
    /// EXECUTE (read `len` as the tag, extract `data[0]`). Returns the fresh OWNED heap
    /// handle `dst` (NOT pushed to `live_heap_handles` — the caller does its own
    /// position-specific bookkeeping). Returns `None` when `value` is not a tracked
    /// Option ctor (a heap-payload `Some`, whose payload is not a lowerable scalar,
    /// falls through here too): the caller then takes its normal deferred-`Opaque` path,
    /// and a `match` over THAT value stays soundly LINEARIZED (it is never in the set).
    ///
    /// `Some(x)` is `Init::OptSome` (len=1, `data[0]`=x); `None` is `Init::Opaque`
    /// (len=0) — the SAME render as today, only now its `dst` is tracked. The ownership
    /// cert is one `Alloc` = i either way (init-agnostic), so NO checker change.
    pub(crate) fn try_lower_option_ctor(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        // Split (2026-07-20, #781 cog>100 burn-down) into per-payload-shape helpers,
        // tried in the SAME ORDER the original single match evaluated its arms (order is
        // load-bearing — Rust's match picks the FIRST arm whose pattern+guard matches, and
        // sequential `.or_else()` over sub-matches run in the same order reproduces that
        // exactly). Pure text move, no logic change — verified via classify_corpus cert
        // byte-identity over the full spec corpus.
        self.try_lower_opt_tuple_and_variant_payloads(value, ty)
            .or_else(|| self.try_lower_opt_heap_general(value, ty))
            .or_else(|| self.try_lower_opt_fallback_and_none(value, ty))
            .or_else(|| self.try_lower_result_ok_heap(value, ty))
            .or_else(|| self.try_lower_result_ok_variant_ctor(value, ty))
            .or_else(|| self.try_lower_result_small_arms(value, ty))
            .or_else(|| self.try_lower_result_err_heap_ok_result(value, ty))
            .or_else(|| self.try_lower_result_err_heap_fallback(value, ty))
    }

}
