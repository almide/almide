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
            // A LAMBDA field (`{ run: (x) => n + ":" + x, name: n }` — the record_fn_field
            // make_handler class): LIFT it to a closure block (the full capture machinery —
            // scalar/heap/Fn/Float captures — builds the self-describing [fnidx][nh|nc<<16][env…]
            // block and pushes it live), which the enclosing aggregate then Consumes into its
            // slot. The record's drop frees it via the generated `__drop_closure` field arm.
            IrExprKind::Lambda { params, body, .. } => {
                let blk = self.lift_lambda(params, body)?;
                if !self.live_heap_handles.contains(&blk) {
                    self.live_heap_handles.push(blk);
                }
                Some(blk)
            }
            // A tracked LOCAL heap var, or a MODULE-LEVEL global (`_style: _default` — the
            // ceangal View ctors): `value_or_global` materializes a global's const/record
            // initializer as a fresh owned cached copy (dropped at scope end); the Dup here
            // gives the aggregate its own distinct reference either way.
            IrExprKind::Var { id } => {
                let src = self.value_or_global(*id).ok()?;
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
                // A variant CONSTRUCTOR element (`(IntV(p), p + 4)` — the gguf read_one
                // tuple-return shape): `IntV` is a registered ctor, NOT a user fn — a plain
                // CallFn would emit a dangling `(call $IntV)` (unlinked). Materialize the
                // fresh OWNED tag-block via `try_lower_variant_ctor` (the same pre-check the
                // list-element arm uses) and track it for the caller's move-in.
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) {
                    let obj = self.try_lower_variant_ctor(expr)?;
                    if !self.live_heap_handles.contains(&obj) {
                        self.live_heap_handles.push(obj);
                    }
                    return Some(obj);
                }
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
            // A `(<flat heap>, <flat heap>)` TUPLE element of a list literal (`[(k, v), …]` —
            // the map.entries / str_str shape, `[Color{r,g,b}: "red"]`'s pairs): a fresh
            // owned tuple block (try_lower_tuple_construct), tracked so the list builder's
            // Consume + retain balances it. Widened from (String,String)/(String,List[
            // scalar]) to ANY pair of ONE-LEVEL-EXACT heap types (String, List[scalar], a
            // flat record, a flat variant) — `Op::DropListStrStr`'s render is purely
            // handle-based (confirmed by reading it), so it frees the pair exactly
            // regardless of which flat-heap kind sits in each slot. GATED so other tuples
            // keep the scalar-only `Record | Tuple` arm below (a bare heap-field tuple
            // still defers there — no leak regression).
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2
                        && is_heap_ty(&tys[0]) && is_heap_ty(&tys[1])
                        && self.is_flat_heap_tuple_slot(&tys[0])
                        && self.is_flat_heap_tuple_slot(&tys[1])) =>
            {
                let obj = self.try_lower_tuple_construct(elements)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A `(<flat heap>, <scalar>)` TUPLE literal (`("a", 1)` — the deep_eq tuple-eq
            // operand / the gguf (key, pos) accumulator element / `[East: 90]`'s pairs):
            // flat-heap slot 0 (@12), scalar slot 1 (@20). try_lower_tuple_construct builds
            // it; the enclosing consumer (an eq operand's cond frame, a list's
            // DropListStrInt) frees the heap slot exactly once.
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && is_heap_ty(&tys[0]) && !is_heap_ty(&tys[1])
                        && self.is_flat_heap_tuple_slot(&tys[0])) =>
            {
                let obj = self.try_lower_tuple_construct(elements)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            // A `(<scalar>, <flat heap>)` TUPLE element of a list literal (`[(i, line)]` —
            // the list.enumerate shape): scalar slot 0 (@12), flat-heap slot 1 (@20).
            // try_lower_tuple_construct builds it (heap mask [1]); it is moved into the
            // enclosing list, whose `$__drop_list_int_str` frees each tuple's heap slot +
            // block, so the tuple's own (harmless) mask never scope-end-fires.
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && is_heap_ty(&tys[1])
                        && self.is_flat_heap_tuple_slot(&tys[1])) =>
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
            // A NESTED SPREAD field (`{ ...v, _style: { ...v._style, width: w } }` — the
            // ceangal modifier class): build the inner record via the SAME spread machinery
            // the bind/tail positions use (base slots Dup-copied, overrides moved in), then
            // route its drop like the nested-record-literal arm above — a heap-nested field
            // frees via the generated recursive `$__drop_<R>`, a scalar-only spread keeps
            // the flat mask (sound: no nested heap). The handle joins `live_heap_handles`
            // so the enclosing aggregate's `Consume` (move-in) + `retain` balances it.
            IrExprKind::SpreadRecord { .. } => {
                let obj = self.try_lower_spread_record_construct(expr)?;
                if let Some(name) = self.record_or_anon_drop_type_name(&expr.ty) {
                    self.record_masks.remove(&obj);
                    self.variant_drop_handles.insert(obj, name);
                }
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
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
            // An Option/Result CTOR field (`Node { val: 5, next: some(10) }` — a record/tuple whose
            // field is `some(..)`/`none`/`ok(..)`/`err(..)`): build the Option/Result block via the
            // shared `try_lower_option_ctor` (a fresh OWNED 0-or-1-element block), then push it to
            // `live_heap_handles` so the enclosing aggregate's per-slot `Consume` (`m`) MOVES it into
            // the slot — exactly the Named-call element's `i`/`m` balance. WITHOUT this arm an
            // Option-ctor field fell to `_ => None` → `try_lower_record_construct` returned None → the
            // whole record degraded to an empty `Alloc{Opaque}` (a later `n.val`/`n.next` read 0).
            IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. } => {
                let obj = self.try_lower_option_ctor(expr, &expr.ty)?;
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
                    use almide_lang::types::constructor::TypeConstructorId;
                    if matches!(ty, Ty::Applied(TypeConstructorId::Option, _)) {
                        self.materialized_options.insert(v);
                        if crate::lower::is_heap_elem_list_ty(ty) {
                            self.heap_elem_lists.insert(v);
                        }
                    } else if crate::lower::is_result_ty(ty) {
                        self.materialized_results.insert(v);
                        if crate::lower::is_heap_elem_list_ty(ty) {
                            self.heap_elem_lists.insert(v);
                        }
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
                    if crate::lower::strict_values() {
                        return Err(crate::lower::strict_const_wall("destructure component"));
                    }
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
            IrPattern::Tuple { elements } => {
                // A tuple-pattern match arm (`match t { (a, b) => … }`) must read each component from
                // its tuple SLOT (base+12+i*8), NOT alias the whole tuple container-grain (which left
                // `a`/`b` reading the tuple pointer / an uninitialized 0). Route through the same
                // layout-aware per-slot loader `let (a, b) = t` uses, when the subject is a tracked
                // materialized aggregate (its slots are real). Falls back to the container-grain
                // recursion below only when there is no per-slot subject (an untracked/None subject).
                if let Some(subj) = subject {
                    if self.materialized_aggregates.contains(&subj) || self.param_values.contains(&subj)
                    {
                        if self.try_lower_tuple_destructure(elements, subj, None) {
                            return Ok(());
                        }
                    }
                }
                for p in elements {
                    self.bind_pattern(p, subject)?;
                }
                Ok(())
            }
            IrPattern::List { elements } => {
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
            // `some(Number(7))` — Some wrapping a CUSTOM-VARIANT ctor payload (the
            // option-of-variant shape): build the variant block, move it into the
            // 1-element Option. Drop routing by the payload's own discipline: a
            // recursive-drop variant routes "optrec:<Type>" → the generated
            // `$__drop_<Type>` frees the payload (fields, then block) then the option
            // block; a flat variant (no heap fields, `Number(Int)`) uses the
            // Some(string) shape — DropListStr's flat slot-0 free IS its exact drop.
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.variant_layouts.ctor_to_type.contains_key(name.as_str())) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind
                else {
                    return None;
                };
                let type_name = self.variant_layouts.ctor_to_type.get(name.as_str())?.clone();
                let needs_rec = self.variant_layouts.needs_recursive_drop(&type_name, &|rn| {
                    crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                });
                let piece = self.try_lower_variant_ctor(expr)?;
                Some(if needs_rec {
                    self.materialize_opt_aggregate_some(piece, repr, type_name)
                } else {
                    self.materialize_opt_str_some(piece, repr)
                })
            }
            // `Some((1, 2))` — an ALL-SCALAR tuple literal payload (`match x { some((a, b))
            // => a + b, … }` — the nested-some-tuple pattern shape). Build the flat tuple
            // block, move it into the 1-element Option: the payload block owns NO inner
            // heap, so DropListStr's flat slot-0 free is EXACT (frees the tuple block, then
            // the option block) — the same discipline as the (Int,String) case above minus
            // the recursive element drop.
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Tuple { elements } = &expr.kind else { return None };
                let elements = elements.clone();
                let piece = self.try_lower_scalar_tuple_construct(&elements)?;
                Some(self.materialize_opt_str_some(piece, repr))
            }
            // `Some((k, v))` — a `(String, <scalar>)` tuple literal payload (`map.find`'s
            // `__skv_find_some(k, v) = Some((kc, v))`). Build the tuple (`try_lower_tuple_
            // construct`, one heap slot: the String), move it into the 1-element Option. The
            // DEFAULT `materialize_opt_str_some` flat drop would only `rc_dec` the TUPLE's
            // own handle, leaking its String (the same class of bug the `_str`-dispatch fix
            // and the drop-authority swap below both guard against) — override the routing
            // to the type-specific recursive `$__drop_opt_str_int` (generated,
            // OPT_STR_INT_DROP_SRC), mirroring the `(Value, scalar)` swap immediately below.
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && !is_heap_ty(&tys[1])) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Tuple { elements } = &expr.kind else { return None };
                let elements = elements.clone();
                let piece = self.try_lower_tuple_construct(&elements)?;
                let obj = self.materialize_opt_str_some(piece, repr);
                self.variant_drop_handles.insert(obj, "opt_str_int".to_string());
                Some(obj)
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
            // `some(<record>)` RETURNED — Option wrapping a heap RECORD (porta find_eq_pos's tail).
            // Materialize the owned record (`try_lower_record_construct`, recursive-drop) and wrap it
            // in the 0-or-1 Option, routing the Option's drop to the recursive `$__drop_<R>`
            // (`Op::DropWrapperRec`) — NOT the flat `DropListStr` that leaks the record's nested heap
            // fields. The tail counterpart of the heap-result-arm record-Some case. Gated on the record
            // needing a recursive drop; a scalar-only record falls through (no `$__drop_<R>`).
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                let repr = repr_of(ty).ok()?;
                let drop_fn = self.record_or_anon_drop_type_name(&expr.ty)?;
                let piece = self.try_lower_record_construct(expr)?;
                Some(self.materialize_opt_aggregate_some(piece, repr, drop_fn))
            }
            // `some(<SCALAR-ONLY record>)` (`some(Point { x: 7, y: 8 })` — compound_repr's
            // opt_rec): the record block owns NO children, so the flat 0-or-1 Option drop
            // (`DropListStr`: rc_dec of the payload handle + the block) frees it EXACTLY —
            // materialize instead of deferring. (The deferred-empty placeholder was silently
            // read as `none` once the container-repr display routed over it — the wrong-bytes
            // class this campaign exists to prevent; construction must be real before display.)
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && matches!(&expr.ty, Ty::Named(..))
                    && self
                        .aggregate_field_tys(&expr.ty)
                        .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self
                    .try_lower_record_construct(expr)
                    .or_else(|| self.try_lower_scalar_record_construct(expr))?;
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
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
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
                    // `Some(Some(..))` / `Some(None)` / `Some(Ok(..))` / `Some(Err(..))` — a NESTED
                    // Option/Result ctor payload. Build the inner Option/Result block recursively
                    // (a fresh OWNED handle), then MOVE it into the outer Some's slot — exactly like
                    // an owned `Var`/Named-call payload. Without this case the nested ctor fell to
                    // `_ => None` and the whole `some(some(42))` degraded to an EMPTY Opaque list
                    // (the nested-Option construction miscompile).
                    IrExprKind::OptionSome { .. }
                    | IrExprKind::OptionNone
                    | IrExprKind::ResultOk { .. }
                    | IrExprKind::ResultErr { .. } => self.try_lower_option_ctor(expr, &expr.ty)?,
                    // `some(p.name)` — a HEAP FIELD projection payload (the optional-chain
                    // `p?.f` desugar's Some arm over a record payload): BORROW the field's
                    // slot handle from the materialized container, `Dup` into a fresh
                    // CO-OWNED ref, and move THAT in — the container keeps its own
                    // reference (freed once by its owner), the wrapper owns the Dup (the
                    // borrowed-param discipline above). Gated to a String field so the
                    // flat materialize_opt_str_some drop is exact.
                    IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let borrow = self.try_lower_heap_field_borrow(expr)?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src: borrow });
                        dup
                    }
                    // A COMPUTED String Some payload (`some("v=" + s)` / `some("v=${x}")`) — the
                    // fresh-owned `__str_concat` chain, operand temps dropped here (the ok/err
                    // ConcatStr/StringInterp arms' Option sibling).
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // A SCALAR-element LIST-literal Some payload (`some([1, 2, 3])`, `some([])`) — build
                    // the fresh owned block (0-length for the empty case, which `try_lower_scalar_list_
                    // construct` declines), moved into the Some slot; `materialize_opt_str_some`'s
                    // heap_elem_lists drop frees it flat (a scalar-element list has no nested ownership).
                    IrExprKind::List { elements }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
                    {
                        self.try_lower_scalar_list_slots(elements)?
                    }
                    // A COMPUTED scalar-element list payload (`some(list.map(xs, f))`, `some([1,2] |> …)`,
                    // `some(a + b)`) — lower the fresh owned list via `lower_owned_heap_field` (which
                    // tracks it in live_heap_handles) then MOVE it into the Some slot (retain-remove so
                    // materialize_opt_str_some is the SOLE owner). Gated to a SCALAR-element list so the
                    // flat heap_elem_lists drop is exact. Without this the computed payload fell to
                    // `_ => None` → a deferred Opaque Option reading `none` (the some(computed) miscompile).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                    | IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `some(string.slice(s, …))` — a PURE Module call yielding a fresh owned
                    // STRING payload (the parse_tag tail-if family): lower_owned_heap_field
                    // routes it via lower_pure_module_value_call; MOVE it into the Some slot
                    // (retain-remove — materialize_opt_str_some is the sole owner, its flat
                    // heap_elem_lists drop frees the one String exactly).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `some((if c then a else b))` — a heap-result IF/MATCH String payload
                    // (fuzz F-858: the un-admitted if fell to the deferred Opaque and the
                    // zeroed option READ `none` — a silent flip). EXECUTE it via the proven
                    // heap-result-if machinery (lower_owned_heap_field's If/Match arms), MOVE
                    // the one owned result into the Some slot. Gated to a String payload so
                    // the flat drop is exact.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // A `Map[String, Int]` (map_skv) Some payload (`some(["a": 1])` → `some(map.from_list
                    // (…))`) — lower the map (a Module call) and MOVE it into the Some slot. The map's own
                    // block is freed by the flat heap_elem_lists drop, exactly as a bare `let m = […]`
                    // (a map_skv block frees like a DynListStr). Gated to the map_skv (String key, scalar
                    // value) layout.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                            if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
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
                // A HEAP-payload Option (`let x: Option[Msg] = none`) ALSO registers the
                // nested-ownership class so a downstream match ADMITS its Some-arm payload
                // bind (heap_or_scalar_bind gates on it); DropListStr over len 0 frees only
                // the block, so the class change is drop-equivalent for a None value.
                if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = ty {
                    if a.len() == 1 && is_heap_ty(&a[0]) {
                        self.heap_elem_lists.insert(dst);
                    }
                }
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
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
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
                    // `ok([])` / `ok(["a", …])` — a LIST-literal Ok payload (the
                    // tail-duplicated `let xs = if c then load(p)! else []` else-arm,
                    // porta resolve_env). The string-list literal builder yields a
                    // fresh owned block movable into the Result exactly like a call
                    // piece; an out-of-subset element list returns None (wall kept).
                    IrExprKind::List { elements } => {
                        let e = (**expr).clone();
                        // A str-element list via the str builder; a SCALAR-element list (`ok([4, 5])`,
                        // List[Int]) via the scalar-slots builder (incl the empty `ok([])`), which the
                        // str builder declines. Both yield a fresh owned block moved into the Ok slot.
                        match self.try_lower_str_list_literal(&e) {
                            Some(obj) => obj,
                            None => self.try_lower_scalar_list_slots(elements)?,
                        }
                    }
                    // `ok("n" + int.to_string(x))` — a COMPUTED String Ok payload (a `ConcatStr`
                    // chain, the `fan.map`/effect-fn `ok(label)` shape). `try_lower_concat_str` yields a
                    // fresh owned `__str_concat` result (movable into the Result exactly like a call
                    // piece); its borrowed operand temps drop here so only the concat result survives to
                    // be consumed by `materialize_result_str`.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `ok(some(5))` / `ok(none)` / `ok(ok(7))` / `ok(err("x"))` — a NESTED Option/Result
                    // ctor Ok payload. Build the inner Option/Result block recursively (a fresh OWNED
                    // handle), moved into the outer Ok's slot — exactly like the OptionSome nested arm.
                    // Without this the nested ctor fell to `_ => None` and the inner degraded to an EMPTY
                    // Opaque (the Result-outer nested-interp `ok(none)` miscompile).
                    IrExprKind::OptionSome { .. }
                    | IrExprKind::OptionNone
                    | IrExprKind::ResultOk { .. }
                    | IrExprKind::ResultErr { .. } => self.try_lower_option_ctor(expr, &expr.ty)?,
                    // A COMPUTED list Ok payload (`ok(list.map(xs, f))`, `ok(a + b)`) — lower the fresh
                    // owned list, moved into the Ok slot (retain-remove so materialize_result_str is the
                    // sole owner). Gated to a SCALAR- or STRING-element list — the two element kinds whose
                    // drop `materialize_result_str` routes exactly (flat for scalar, per-element String
                    // free for List[String], the same as the `ok(["a", …])` literal path). Mirrors the
                    // OptionSome computed arm; without it `ok(computed)` fell to a deferred Opaque `ok([])`.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                    | IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && (!is_heap_ty(&a[0]) || matches!(a[0], Ty::String))) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // A `Map[String, Int]` (map_skv) Ok payload (`ok(["a": 1])` → `ok(map.from_list(…))`)
                    // — mirror the OptionSome map arm: the flat drop frees the map_skv block.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                            if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `ok(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned STRING
                    // Ok payload (fuzz C-class 323/768: the un-admitted stdlib call fell to the
                    // deferred Opaque and the ZEROED block printed `ok("")` — a silent wrong value).
                    // `lower_owned_heap_field` routes it via `lower_pure_module_value_call` (purity/
                    // HOF gates apply there); MOVE it into the Ok slot (retain-remove — the
                    // materialized Result is the sole owner, its flat DropListStr slot-0 free is
                    // exact, the same discipline as the Named-call String piece above).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `ok((if c then a else b))` — a heap-result IF/MATCH String Ok payload
                    // (the fuzz F-858 family's Result sibling): the heap-result-if machinery
                    // yields the one owned result, moved into the Ok slot.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_result_str(piece, repr, false, false);
                // TRACK the bound Result like every other materialized producer —
                // without this a later `match $t { ok/err }` over the LET-BOUND var
                // was UNTRACKED and rolled back (the monadic-desugar else-arm
                // `let $t = ok([]); match $t` — porta resolve_env walled on it).
                self.seed_variant_param(dst, ty);
                Some(dst)
            }
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_ok(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            // `err(<scalar>)` for a SCALAR-SCALAR Result (`Result[Int, Int]` — the
            // match_container `ck(err(404))` class): the len-as-tag Err twin of the
            // scalar-Ok arm above. NOT heap_elem_lists-tracked — the flat Drop is the
            // exact free (a DropListStr over len 1 would rc_dec the raw scalar payload
            // as a handle). Gated to BOTH sides scalar so the heap-err layouts (String
            // err → DropListStr slot-0 free) keep their existing arms below.
            IrExprKind::ResultErr { expr }
                if !is_heap_ty(&expr.ty)
                    && matches!(ty,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                            if a.len() == 2 && !is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
            {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_err_scalar(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            // `err(<user-variant ctor>)` for `Result[T_scalar, <user variant>]` — the
            // structured-error class (`let e: Result[Int, MathError] =
            // err(Overflow("m"))`, `assert_eq(f(x), err(DivideByZero))`). The
            // len-as-tag Err wrapper the reader seeds for this type; rich payloads
            // route to `$__drop_res_<V>`. NOT self-tracked: like every ctor arm here,
            // the CALLER owns the tracking (a call-arg site re-tracks via
            // `materialized_call_arg` — a push here double-freed at scope end).
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_variant_err_result(ty) => {
                self.try_lower_result_err_variant_ctor(value, ty)
            }
            // `err(<user-variant ctor>)` for `Result[T_heap, <user variant>]` — the HEAP-Ok
            // structured-error class (`assert_eq(classify(-3), err(NegativeInput(-3)))`).
            // Cap-as-tag wrapper; a rich payload routes to the Err-side recursion
            // (`reserr:<V>`). MUST precede the generic both-heap Err arm below, whose
            // Named-call fallback emitted the ctor as a dangling `(call $NegativeInput)`.
            // A `err(<Var>)` payload keeps the generic route (owned-move / param-Dup).
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty)
                    && !matches!(&expr.kind, IrExprKind::Var { .. })
                    && self.is_heap_ok_variant_err_result(ty) =>
            {
                self.try_lower_result_err_variant_ctor_heap_ok(value, ty)
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
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
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
                    // `err("bad " + reason)` — a COMPUTED String Err payload (`ConcatStr`). Same
                    // fresh-owned `__str_concat` piece as an `ok(concat)`; operand temps drop here.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `err(["a", "b"])` — a `List[String]` LITERAL payload (the result.collect
                    // Err side, `Result[List[Int], List[String]]`): the inner list builds
                    // fresh-owned; the Result block's flat DropListStr would free slot-0 as a
                    // STRING (leaking the inner list's elements), so RECLASSIFY the drop below
                    // to the recursive list-of-list-str free.
                    IrExprKind::List { .. }
                        if matches!(&expr.ty,
                            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, i)
                                if i.len() == 1 && matches!(i[0], Ty::String)) =>
                    {
                        let obj = self.try_lower_str_list_literal(expr)?;
                        let dst = self.materialize_result_str(obj, repr, true, false);
                        self.heap_elem_lists.remove(&dst);
                        self.list_list_str_lists.insert(dst);
                        return Some(dst);
                    }
                    // `err(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned
                    // STRING Err payload (fuzz C-class: fell to the deferred Opaque whose zeroed
                    // block even flipped the TAG — printed `ok("")` for an err). Same piece as the
                    // ok-side Module-call arm; the cap-as-tag Err slot owns the one String.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `err((if c then a else b))` — a heap-result IF/MATCH String Err payload
                    // (the F-858 family): the one owned result moves into the Err slot.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
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
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
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
                    // A COMPUTED String Err payload (`ConcatStr`) — fresh-owned concat piece.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `err(float.to_fixed(x, 4))` for a SCALAR-Ok Result — a PURE Module call
                    // yielding a fresh owned STRING Err payload (fuzz C-class, len-as-tag twin of
                    // the heap-Ok Module-call arms): the deferred Opaque zeroed the block. Same
                    // fresh-owned move-in as the Named-call piece above.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `err((if c then a else b))` for a SCALAR-Ok Result — the heap-result
                    // IF/MATCH String Err payload (the F-858 family, len-as-tag twin).
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_opt_str_some(piece, repr);
                // materialize_opt_str_some registers the OPTION read-shape; this value is
                // a RESULT (len-as-tag, Err = len 1) — a reader that keeps both entries
                // resolves it as an Option (`is_result = results ∧ ¬options`) and takes
                // the Err payload as a Some payload (`err("x") ?? 0` returned the String
                // HANDLE — result_option_matrix's "if with ??"). Result-only tracking.
                self.materialized_options.remove(&dst);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            _ => None,
        }
    }
}
