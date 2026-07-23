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
            // A `(String, <Fn>)` TUPLE literal (`("a", () => …)` — the closure-valued
            // map's from_list pair): String slot 0, closure-block slot 1 (the Lambda
            // element lifts via this fn's own Lambda arm inside
            // `try_lower_tuple_construct`). The enclosing pairs list frees it via
            // `$__drop_list_str_clo` (key rc_dec + `__drop_closure` per value).
            IrExprKind::Tuple { elements }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2
                        && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Fn { .. })) =>
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
            .or_else(|| self.try_lower_result_small_arms(value, ty))
            .or_else(|| self.try_lower_result_err_heap_ok_result(value, ty))
            .or_else(|| self.try_lower_result_err_heap_fallback(value, ty))
    }
}
