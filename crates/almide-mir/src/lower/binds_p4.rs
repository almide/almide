impl LowerCtx {
    /// Lower a record/tuple field EXPRESSION whose type is HEAP to a FRESH OWNED handle the
    /// aggregate will own (moved into its slot). The admitted kinds mirror
    /// [`Self::try_lower_str_list_literal`]'s element kinds:
    /// - a `LitStr` is a fresh `Alloc{Str}` (cert `i`);
    /// - a `BinOp::ConcatStr` is the self-host `__str_concat` CallFn (cert `i`);
    /// - a tracked heap `Var` gets its OWN reference via `Dup` (cert `a`) so the original
    ///   binding keeps its reference (no double-free) and the aggregate owns a distinct one.
    /// Any other kind (a heap-returning call, a member access, a nested record literal)
    /// defers — `None`. The returned handle is in `live_heap_handles`; the caller MUST
    /// `Consume` it (the move-in) and remove it from the live set.
    pub(crate) fn lower_owned_heap_field(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        match &expr.kind {
            IrExprKind::LitStr { value: s } => {
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: obj,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(s.clone()),
                });
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => {
                let obj = self.try_lower_concat_str(expr)?;
                // try_lower_concat_str returns a fresh owned String (a CallFn result); track it
                // so the caller's Consume + live-set removal balances it.
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A list CONCAT field (`children: parent.children + [child]` — the svg add_child spread
            // override): a fresh owned list (`__list_concat`/`_rc`), the new record co-owns it; the
            // result's per-element drop tracking is set by try_lower_concat_list (incl List[Record]).
            IrExprKind::BinOp { op: BinOp::ConcatList, .. } => {
                let obj = self.try_lower_concat_list(expr)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            IrExprKind::StringInterp { parts } => {
                let obj = self.try_lower_string_interp(parts)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            IrExprKind::Var { id } => {
                let src = *self.value_of.get(id)?;
                let dup = self.fresh_value();
                self.ops.push(Op::Dup { dst: dup, src });
                self.live_heap_handles.push(dup);
                Some(dup)
            }
            // A HEAP FIELD ACCESS field (`{ key: state.key, … }` — the aes cfb8 nested record copies
            // its key/expanded_key from the `state` PARAM): BORROW the source slot's handle
            // (`LoadHandle` of `container_handle + slot_offset`, the still-owning param keeps its
            // reference) then `Dup` it so the new aggregate owns a DISTINCT reference (cert `a`).
            // Same borrow-then-Dup the spread-record copy (`try_lower_spread_record_construct`) and
            // the tuple-element borrow use — no double-free (the source param's masked drop frees its
            // own ref; the new aggregate's drop frees the Dup'd one). Defers (`None`) for an
            // unresolvable container (`f().key`) or a non-heap slot (the scalar path owns those).
            IrExprKind::Member { object, field } => {
                let offset = self.aggregate_field_offset_any(&object.ty, field.as_str())?;
                let h = self.resolve_aggregate_container_handle(object)?;
                Some(self.dup_borrowed_slot(h, offset))
            }
            IrExprKind::TupleIndex { object, index } => {
                let offset = self.aggregate_index_offset_any(&object.ty, *index)?;
                let h = self.resolve_aggregate_container_handle(object)?;
                Some(self.dup_borrowed_slot(h, offset))
            }
            // A user-call element (`(parse_inline(after), pos + 1)` — the dominant yaml tuple shape):
            // the callee returns a FRESH owned heap value (CallFn result = cert `i`, rc 1), tracked
            // so the enclosing tuple's per-slot `Consume` (`m`) moves it into the slot — the tuple
            // then owns it (its masked recursive DropListStr frees it). Same `i`/`m` balance as the
            // Var element's Dup. A pure Module-call (`value.array(items)`) returns a fresh Value the
            // same way; an impure/HO callee errors → None → the tuple defers (sound Opaque).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&expr.ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(obj),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let obj = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty)
                    .ok()?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A `List[Int/Float/Bool]` LITERAL field (`{ items: [1, 2, 3] }`, `{ items: [] }`) —
            // materialize the scalar-element block (flat slots, no nested ownership) as a fresh
            // OWNED list. The aggregate owns it; its masked recursive drop `rc_dec`s the block
            // (sound: scalar elements need no per-element free). An EMPTY scalar list is a valid
            // 0-length block (so `{ items: [] }` materializes, not Opaque-with-garbage).
            //
            // A NON-EMPTY heap-element list field — a `List[Record]` (`children: [rect(…), …]`,
            // via the record-list builder) OR a `List[String]` (`words: ["if", "then", …]`, via the
            // str-list builder) — materializes as a fresh OWNED nested-ownership block the aggregate
            // owns. The enclosing record's generated `$__drop_<R>` frees this field RECURSIVELY
            // (`__drop_list_<E>` / `__drop_list_str` of the slot handle — see
            // `record_drop_field_frees`, which routes a `List[heap]` field to its recursive list
            // drop, NOT the flat one-level mask), so no leak. The handle is pushed to
            // `live_heap_handles` so the caller's `Consume` (move-in) + `retain` balances it.
            IrExprKind::List { elements } => {
                use almide_lang::types::constructor::TypeConstructorId;
                let scalar_list = matches!(&expr.ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
                if !scalar_list && !elements.is_empty() {
                    // A `List[Record]` / `List[(String,String)]` field — the record-list builder
                    // already tracks it in `live_heap_handles` + routes its drop (`$__drop_list_<R>`
                    // / `DropListStrStr`), so return it directly (do NOT re-push).
                    if let Some(obj) = self.try_lower_record_list_literal(expr) {
                        return Some(obj);
                    }
                    // A `List[String]` (and the other heap-element list shapes the str-list builder
                    // admits) field — materialize the real nested-ownership block. The builder
                    // registers the recursive drop set + `materialized_lists` but does NOT push to
                    // `live_heap_handles`, so push it here for the caller's move-in to balance.
                    if let Some(obj) = self.try_lower_str_list_literal(expr) {
                        self.live_heap_handles.push(obj);
                        return Some(obj);
                    }
                    // Any other non-record heap-element list field is the recursive frontier → defer.
                    return None;
                }
                let obj = self.try_lower_scalar_list_slots(elements)?;
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            // A `(String, String)` TUPLE element of a list literal (`[(k, v), …]` — the map.entries /
            // str_str shape): a fresh owned tuple block (try_lower_tuple_construct), tracked so the
            // list builder's Consume + retain balances it. GATED to (String,String) so other tuples
            // keep the scalar-only `Record | Tuple` arm below (a bare heap-field tuple still defers
            // there — no leak regression).
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::String)) =>
            {
                let obj = self.try_lower_tuple_construct(elements)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // An `(Int, String)` TUPLE element of a list literal (`[(i, line)]` — the list.enumerate
            // shape): Int slot 0 (scalar @12), String slot 1 (heap @20). try_lower_tuple_construct
            // builds it (heap mask [1]); it is moved into the enclosing list, whose `$__drop_list_int_str`
            // frees each tuple's String + block, so the tuple's own (harmless) mask never scope-end-fires.
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                let obj = self.try_lower_tuple_construct(elements)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // brick 3: a (Value, scalar) TUPLE — the yaml/cm2 effect-fn tuple-RESULT shape
            // `(value.object(pairs), pos)`. Slot 0 is a Value (heap, @12), slot 1 a scalar (@20).
            // `try_lower_tuple_construct` builds it + records `record_masks[obj] = [0]`, but a FLAT
            // per-slot rc_dec of the Value slot would LEAK the Value's nested Array/Object payload. So
            // SWAP that for the recursive `$__drop_value_tuple` (value_core) by removing the flat mask
            // and routing the drop through `variant_drop_handles="value_tuple"` (→ `DropVariant`).
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && crate::lower::is_value_ty(&tys[0]) && !is_heap_ty(&tys[1])) =>
            {
                let obj = self.try_lower_tuple_construct(elements)?;
                self.record_masks.remove(&obj);
                self.variant_drop_handles.insert(obj, "value_tuple".to_string());
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // An empty Map field — `attrs: [:]` (the svg `el` record). A v1 Map is a List block of
            // paired slots; an EMPTY one is the same layout-agnostic 0-length block as an empty list.
            // (A non-empty Map literal as a record field is a later brick.)
            IrExprKind::EmptyMap => {
                let obj = self.try_lower_scalar_list_slots(&[])?;
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            IrExprKind::MapLiteral { entries } if entries.is_empty() => {
                let obj = self.try_lower_scalar_list_slots(&[])?;
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            // A NESTED RECORD LITERAL field that is itself a recursive-drop record — a NAMED one
            // (`{ data, state: Cfb8State { … } }` — Cfb8State owns Bytes fields) or an ANONYMOUS one
            // that owns heap (`{ st: { iv: Bytes } }`) — build it RECURSIVELY via
            // `try_lower_record_construct` (which moves each heap field in + sets its own
            // `record_masks`), and route its drop to the generated `__drop_<R>` /
            // `__drop_anonrec_<hash>` so a heap-IN-nested field is freed by the inner record's OWN
            // recursive drop, NOT a flat one-level mask `rc_dec` (which would leak it). The outer
            // record then routes ITS drop of this field through the same `__drop_…` (the field's
            // slot is tagged in the outer's synthesized recursive drop — see
            // `collect_recursive_anon_records` / `record_drop_field_frees`). The borrow-by-default
            // cert is unchanged: the inner is `i…m` (alloc + move-in), and its `d` is the recursive
            // `__drop_…` (a trusted prim-only routine, leak-loop verified).
            IrExprKind::Record { .. }
                if self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                let obj = self.try_lower_record_construct(expr)?;
                if let Some(name) = self.record_or_anon_drop_type_name(&expr.ty) {
                    self.variant_drop_handles.insert(obj, name);
                }
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A NESTED RECORD/TUPLE LITERAL field (`Outer { p: Point { x: 1, y: 2 }, n: 5 }`) —
            // materialize the inner block as a fresh OWNED aggregate the outer owns. Its own
            // construction (scalar / mixed-heap) registers it in `materialized_aggregates`, so
            // the recursive `${outer}` Display reads the inner's real slots. The outer's masked
            // drop `rc_dec`s the inner block; if the INNER has heap fields of its OWN, those are
            // freed by the inner block's own mask — but the outer mask only `rc_dec`s the inner
            // BLOCK (one level), so a heap-IN-nested field would leak. To stay sound, admit a
            // nested aggregate ONLY when it is SCALAR-only (no nested heap to leak) — the
            // recursive-drop NAMED-record case above handles a heap-nested NAMED record; an
            // ANONYMOUS heap-nested aggregate (no `__drop_<R>` to route through) defers (`None`)
            // → the outer walls (never wrong bytes, never a leak).
            IrExprKind::Record { .. } | IrExprKind::Tuple { .. } => {
                let scalar_only = self
                    .aggregate_field_tys(&expr.ty)
                    .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t)));
                if !scalar_only {
                    return None;
                }
                let obj = match &expr.kind {
                    IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(expr)?,
                    IrExprKind::Tuple { elements } => self.try_lower_scalar_tuple_construct(elements)?,
                    _ => return None,
                };
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            // A heap-result `if`/`match` ELEMENT (`(if retries == 0 then "pass-1shot" else "pass-retry",
            // "")` — the dojo `classify` tuple-result-if shape). EXECUTE it via the proven heap-result-`if`
            // machinery: each arm `Alloc`s + `Consume`s its value (the per-arm `"im"` move-out balance),
            // and the merged `IfThen` `dst` is the ONE owned rc=1 result (whichever arm ran). Push it to
            // `live_heap_handles` so the enclosing tuple's per-slot `Consume` (`m`) + `retain` MOVES it
            // into the slot — exactly the Named-call / concat element's `i`/`m` balance, UNCONDITIONAL
            // (a value is always produced, both arms), so flat-cert-provable. A `match` desugars to the
            // nested-`if` chain first; an out-of-subset arm rolls back (`None`) → the tuple defers.
            IrExprKind::If { cond, then, else_ } => {
                let obj = self.try_lower_heap_result_if(cond, then, else_, &expr.ty)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            IrExprKind::Match { subject, arms } => {
                let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
                let IrExprKind::If { cond, then, else_ } = &if_expr.kind else {
                    return None;
                };
                let obj = self.try_lower_heap_result_if(cond, then, else_, &expr.ty)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            _ => None,
        }
    }

    /// BORROW the heap handle at `container_handle + offset` (`LoadHandle` — the container keeps its
    /// own reference) then `Dup` it to a fresh OWNED reference (cert `a`) the caller's aggregate
    /// owns. Tracked in `live_heap_handles` so the caller's `Consume` (move-in) balances it. The
    /// borrow+Dup pair is the SAME machinery `try_lower_spread_record_construct` uses for a copied
    /// heap field — no double-free (two distinct refs, two distinct drops).
    fn dup_borrowed_slot(&mut self, container_handle: ValueId, offset: u32) -> ValueId {
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

    /// Shared block-builder for a scalar tuple/list: lower each element to a scalar value, alloc a
    /// `DynList` of `n` i64 slots, `store64` each. Element ownership-free (scalars), flat drop.
    fn try_lower_scalar_list_slots(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None;
        }
        // Lower each field's scalar value first (before the alloc, so a field expr that itself
        // allocates doesn't interleave with our store sequence).
        let vals: Vec<ValueId> = elements
            .iter()
            .map(|e| self.lower_scalar_value(e))
            .collect::<Option<Vec<_>>>()?;
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: elements.len() as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (i, v) in vals.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, v] });
        }
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
    fn try_lower_tuple_destructure(&mut self, pats: &[IrPattern], subject: ValueId) -> bool {
        use crate::{IntOp, PrimKind};
        // Does the subject OWN its heap slots (a tracked masked aggregate) OR is it a borrow whose
        // owner is elsewhere (a param / a borrowed element handle)? Either way a per-slot HEAP read
        // is a sound borrow. An untracked owned heap subject would leak the borrowed inner block, so
        // a heap field over it must defer to the container-grain alias.
        let heap_borrow_ok =
            self.materialized_aggregates.contains(&subject) || self.param_values.contains(&subject);
        for p in pats {
            match p {
                IrPattern::Bind { ty, .. } if !is_heap_ty(ty) => {}
                IrPattern::Bind { .. } => {
                    if !heap_borrow_ok {
                        return false;
                    }
                }
                IrPattern::Wildcard => {}
                _ => return false,
            }
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subject] });
        for (i, p) in pats.iter().enumerate() {
            if let IrPattern::Bind { var, ty } = p {
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
                } else {
                    self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
                }
                self.value_of.insert(*var, v);
            }
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
            IrPattern::Bind { var, ty } => {
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
                    self.ops.push(Op::Const { dst });
                }
                self.value_of.insert(*var, dst);
                Ok(())
            }
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                self.bind_pattern(inner, subject)
            }
            IrPattern::Constructor { args, .. } => {
                for p in args {
                    self.bind_pattern(p, subject)?;
                }
                Ok(())
            }
            IrPattern::Tuple { elements } | IrPattern::List { elements } => {
                for p in elements {
                    self.bind_pattern(p, subject)?;
                }
                Ok(())
            }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(p) => self.bind_pattern(p, subject)?,
                        None => {
                            return Err(LowerError::Unsupported(
                                "record pattern shorthand field (no bound VarId) not in this brick".into(),
                            ))
                        }
                    }
                }
                Ok(())
            }
        }
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
        match &value.kind {
            // `Some(heap)` RETURNED / bound directly — a fresh OWNED message/element (a LitStr, a
            // Named-call result, or an OWNED `Var` in `live_heap_handles`, NOT a borrowed param)
            // materializes the 0-or-1-element DynListStr Option (the element MOVED in). Same cert as
            // the heap-result-`if` arm; the owned gate keeps a borrowed `Some(param)` deferred.
            // `Some(Value)` — a dynamic Value payload (`list.get_value`'s `Some(@i)`): Dup the
            // (borrowed) Value into a fresh co-owned ref via `lower_owned_heap_field` (exactly
            // value.get's `Ok(@12)`), then materialize the 0-or-1 Option. The flat rc_dec drop
            // (heap_elem_lists) is correct — the Value is CO-OWNED (the list keeps its ref; the shared
            // block is recursively freed at the LAST ref, via the list's own drop). Checked before the
            // general heap-Some arm, whose Var case requires `live_heap_handles` (a borrow is not).
            // `Some((Int, String))` — the `list.find` over a `List[(Int,String)]` result. Co-own the
            // (borrowed) tuple by Dup (`lower_owned_heap_field`), then materialize a 1-element Option
            // whose drop is the RECURSIVE `$__drop_list_int_str` (the per-tuple rc==1 guard makes the
            // co-ownership with the source list safe — no leak, no double-free).
            IrExprKind::OptionSome { expr }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self.lower_owned_heap_field(expr)?;
                Some(self.materialize_opt_int_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr }
                if crate::lower::is_value_ty(&expr.ty)
                    || matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String)) =>
            {
                // `Some(Value)` (list.get_value) OR `Some(List[String])` (list.get_liststr over a
                // List[List[String]]): share a NESTED-heap element by handle — Dup the borrowed element
                // into a co-owned ref (`lower_owned_heap_field`), materialize the 0-or-1 Option. The flat
                // rc_dec drop is correct (co-owned; the source list keeps its ref and frees the shared
                // block at the last ref via its own drop).
                let repr = repr_of(ty).ok()?;
                let piece = self.lower_owned_heap_field(expr)?;
                Some(self.materialize_opt_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                // materialize_opt_str_some tracks materialized_options + heap_elem_lists.
                Some(self.materialize_opt_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr } => {
                // SCALAR payload only — `lower_scalar_value` returns `None` for a heap
                // payload, which IS the gate (a heap `Some` aliases its element, a later
                // refinement; it falls through to the deferred `Opaque` path, untracked).
                let payload = self.lower_scalar_value(expr)?;
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptSome { payload } });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            IrExprKind::OptionNone => {
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                // `None` is the 0-element Option, sized like `OptSome` (`Init::OptNone`) so the
                // free-list reuses a block between Some/None results; tracked as materialized.
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptNone });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            // A `Result[Int, String]` ctor RETURNED / bound directly (`fn f() = Ok(y)` / `… = Err(
            // msg)`) MATERIALIZES the DynListStr Result (len-as-tag: Ok = len 0 with the scalar in
            // slot 0, Err = len 1 owning the message), tracked so the caller can `match` it. Same
            // cert as the heap-result-`if` arms (reuses `materialize_result_ok` / the Some-string
            // builder) — no new Init. SCALAR Ok payload, heap (Var/LitStr/Named-call) Err payload.
            // HEAP-Ok `Result[String, String]` (`Ok(s)` with a heap payload, both arms heap) RETURNED
            // / bound directly — the 2-SLOT DynListStr (String @slot 0, Ok/Err tag @slot 1, len 1 so
            // `DropListStr` frees only the one String). Same cert as the Err-heap arm (one owned
            // String moved in). Owned-`Var` / LitStr / Named-call piece only (a borrowed param would
            // double-free), else the deferred Opaque.
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                Some(self.materialize_result_str(piece, repr, false, false))
            }
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_ok(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            // HEAP-Ok `Result[(Int,Int), String]` etc. — `Err(msg)` RETURNED / bound directly
            // (`fn __rzip_err(..) = Err(copy)`). The Err message goes into the SAME cap-as-tag 1-slot
            // DynListStr as the heap-Ok arm (payload @12, tag @16 = 1), so a `match` reading tag @16
            // sees Err. Without this it would fall to the len-as-tag arm below (a DIFFERENT layout the
            // heap-Ok match misreads). Owned-`Var` / LitStr / Named-call piece only.
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                Some(self.materialize_result_str(piece, repr, true, false))
            }
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                // A FRESH owned message only — a LitStr alloc, a Named-call result, or an OWNED
                // `Var` (one in `live_heap_handles` — a freshly-built/closure-returned String, NOT
                // a BORROWED param). Consuming a borrow into the Err would move out a value the
                // caller still owns (a double-free the checker rejects), so a borrowed `Var` falls
                // through to the sound deferred `Opaque`.
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        self.value_for(*id).ok()?
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_opt_str_some(piece, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            _ => None,
        }
    }
}
