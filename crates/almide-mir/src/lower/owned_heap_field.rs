// The OWNED-HEAP-FIELD family: lower a record/tuple field expression whose type
// is heap to a fresh owned handle the aggregate moves into its slot — the leaf,
// aggregate, and call legs. Split out of binds_p4.rs (max-lines, #852); the
// impl block's first four methods moved verbatim.

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
        if let Some(v) = self.lower_owned_heap_field_leaf(expr) { return v; }
        if let Some(v) = self.lower_owned_heap_field_aggregate(expr) { return v; }
        if let Some(v) = self.lower_owned_heap_field_call(expr) { return v; }
        None
    }

    /// Variables, literals and the direct constructors.
    ///
    /// One group of `lower_owned_heap_field`'s arm table, arms verbatim and in
    /// source order. The two negatives are DISTINCT and must stay so: `None`
    /// means "not my group", so the router tries the next one, while
    /// `Some(None)` is an arm DECLINING the field — what the single table's
    /// `_ => None` meant. Collapsing them makes a declined field fall through to
    /// a later group and lower by the wrong rule.
    fn lower_owned_heap_field_leaf(&mut self, expr: &IrExpr) -> Option<Option<ValueId>> {
        use almide_ir::BinOp;
        Some(match &expr.kind {
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
                let src = match self.value_or_global(*id) {
                    Ok(v) => v,
                    Err(e) => {
                        crate::trace::trace("ALMIDE_DBG_ELEM", || {
                            format!("[heap-field] Var {id:?} value_or_global declined: {e:?}")
                        });
                        return None;
                    }
                };
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
            _ => return None,
        })
    }

    /// Records, tuples, lists and the branching forms.
    ///
    /// One group of `lower_owned_heap_field`'s arm table, arms verbatim and in
    /// source order. The two negatives are DISTINCT and must stay so: `None`
    /// means "not my group", so the router tries the next one, while
    /// `Some(None)` is an arm DECLINING the field — what the single table's
    /// `_ => None` meant. Collapsing them makes a declined field fall through to
    /// a later group and lower by the wrong rule.
    fn lower_owned_heap_field_aggregate(&mut self, expr: &IrExpr) -> Option<Option<ValueId>> {
        Some(match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                return self.lower_named_call_heap_field(expr, name.as_str(), args);
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
                return self.lower_list_literal_heap_field(expr, elements);
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
            IrExprKind::Tuple { elements } if self.is_flat_heap_pair_ty(&expr.ty) => {
                self.build_tracked_tuple(elements)?
            }
            // A `(String, <Fn>)` TUPLE literal (`("a", () => …)` — the closure-valued
            // map's from_list pair): String slot 0, closure-block slot 1 (the Lambda
            // element lifts via this fn's own Lambda arm inside
            // `try_lower_tuple_construct`). The enclosing pairs list frees it via
            // `$__drop_list_str_clo` (key rc_dec + `__drop_closure` per value).
            IrExprKind::Tuple { elements } if is_str_closure_pair_ty(&expr.ty) => {
                self.build_tracked_tuple(elements)?
            }
            // A `(<flat heap>, <scalar>)` TUPLE literal (`("a", 1)` — the deep_eq tuple-eq
            // operand / the gguf (key, pos) accumulator element / `[East: 90]`'s pairs):
            // flat-heap slot 0 (@12), scalar slot 1 (@20). try_lower_tuple_construct builds
            // it; the enclosing consumer (an eq operand's cond frame, a list's
            // DropListStrInt) frees the heap slot exactly once.
            IrExprKind::Tuple { elements } if self.is_flat_heap_scalar_pair_ty(&expr.ty) => {
                self.build_tracked_tuple(elements)?
            }
            // A `(<scalar>, <flat heap>)` TUPLE element of a list literal (`[(i, line)]` —
            // the list.enumerate shape): scalar slot 0 (@12), flat-heap slot 1 (@20).
            // try_lower_tuple_construct builds it (heap mask [1]); it is moved into the
            // enclosing list, whose `$__drop_list_int_str` frees each tuple's heap slot +
            // block, so the tuple's own (harmless) mask never scope-end-fires.
            IrExprKind::Tuple { elements } if self.is_scalar_flat_heap_pair_ty(&expr.ty) => {
                self.build_tracked_tuple(elements)?
            }
            // brick 3: a (Value, scalar) TUPLE — the yaml/cm2 effect-fn tuple-RESULT shape
            // `(value.object(pairs), pos)`. Slot 0 is a Value (heap, @12), slot 1 a scalar (@20).
            // `try_lower_tuple_construct` builds it + records `record_masks[obj] = [0]`, but a FLAT
            // per-slot rc_dec of the Value slot would LEAK the Value's nested Array/Object payload. So
            // SWAP that for the recursive `$__drop_value_tuple` (value_core) by removing the flat mask
            // and routing the drop through `variant_drop_handles="value_tuple"` (→ `DropVariant`).
            IrExprKind::Tuple { elements } if is_value_scalar_pair_ty(&expr.ty) => {
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
            _ => return None,
        })
    }

    /// The `Named`-callee field: a registered variant CONSTRUCTOR materializes its fresh
    /// owned tag-block; any other name is a real user fn call whose heap result the
    /// aggregate owns. Extracted verbatim from [`Self::lower_owned_heap_field_aggregate`]
    /// (codopsy round-3 sweep, #852).
    fn lower_named_call_heap_field(
        &mut self,
        expr: &IrExpr,
        name: &str,
        args: &[IrExpr],
    ) -> Option<Option<ValueId>> {
        Some({
                // A variant CONSTRUCTOR element (`(IntV(p), p + 4)` — the gguf read_one
                // tuple-return shape): `IntV` is a registered ctor, NOT a user fn — a plain
                // CallFn would emit a dangling `(call $IntV)` (unlinked). Materialize the
                // fresh OWNED tag-block via `try_lower_variant_ctor` (the same pre-check the
                // list-element arm uses) and track it for the caller's move-in.
                if self.variant_layouts.ctor_to_type.contains_key(name) {
                    let obj = self.try_lower_variant_ctor(expr)?;
                    if !self.live_heap_handles.contains(&obj) {
                        self.live_heap_handles.push(obj);
                    }
                    return Some(Some(obj));
                }
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&expr.ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(obj),
                    name: name.to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.live_heap_handles.push(obj);
                Some(obj)
        })
    }

    /// The `List` LITERAL field. Extracted verbatim from
    /// [`Self::lower_owned_heap_field_aggregate`] (codopsy round-3 sweep, #852).
    fn lower_list_literal_heap_field(
        &mut self,
        expr: &IrExpr,
        elements: &[IrExpr],
    ) -> Option<Option<ValueId>> {
        Some({
                use almide_lang::types::constructor::TypeConstructorId;
                let scalar_list = matches!(&expr.ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
                if !scalar_list && !elements.is_empty() {
                    // A `List[Record]` / `List[(String,String)]` field — the record-list builder
                    // already tracks it in `live_heap_handles` + routes its drop (`$__drop_list_<R>`
                    // / `DropListStrStr`), so return it directly (do NOT re-push).
                    if let Some(obj) = self.try_lower_record_list_literal(expr) {
                        return Some(Some(obj));
                    }
                    // A `List[String]` (and the other heap-element list shapes the str-list builder
                    // admits) field — materialize the real nested-ownership block. The builder
                    // registers the recursive drop set + `materialized_lists` but does NOT push to
                    // `live_heap_handles`, so push it here for the caller's move-in to balance.
                    if let Some(obj) = self.try_lower_str_list_literal(expr) {
                        self.live_heap_handles.push(obj);
                        return Some(Some(obj));
                    }
                    // Any other non-record heap-element list field is the recursive frontier → defer.
                    return Some(None);
                }
                let obj = self.try_lower_scalar_list_slots(elements)?;
                self.live_heap_handles.push(obj);
                Some(obj)
        })
    }

    /// Build a TUPLE field's fresh owned block and track it for the caller's move-in (the
    /// builder may already have tracked it — never push twice). The shared body of the
    /// four flat-slot tuple arms. Extracted verbatim from
    /// [`Self::lower_owned_heap_field_aggregate`] (codopsy round-3 sweep, #852).
    fn build_tracked_tuple(&mut self, elements: &[IrExpr]) -> Option<Option<ValueId>> {
        let obj = self.try_lower_tuple_construct(elements)?;
        if !self.live_heap_handles.contains(&obj) {
            self.live_heap_handles.push(obj);
        }
        Some(Some(obj))
    }

    /// A pair of ONE-LEVEL-EXACT heap slots — the `Op::DropListStrStr` handle-based free
    /// covers either slot kind. Extracted verbatim from
    /// [`Self::lower_owned_heap_field_aggregate`]'s arm guard (codopsy round-3 sweep, #852).
    fn is_flat_heap_pair_ty(&self, ty: &Ty) -> bool {
        matches!(ty,
            Ty::Tuple(tys) if tys.len() == 2
                && is_heap_ty(&tys[0]) && is_heap_ty(&tys[1])
                && self.is_flat_heap_tuple_slot(&tys[0])
                && self.is_flat_heap_tuple_slot(&tys[1]))
    }

    /// A `(<flat heap>, <scalar>)` pair: flat-heap slot 0 (@12), scalar slot 1 (@20).
    /// Extracted verbatim from [`Self::lower_owned_heap_field_aggregate`]'s arm guard
    /// (codopsy round-3 sweep, #852).
    fn is_flat_heap_scalar_pair_ty(&self, ty: &Ty) -> bool {
        matches!(ty,
            Ty::Tuple(tys) if tys.len() == 2 && is_heap_ty(&tys[0]) && !is_heap_ty(&tys[1])
                && self.is_flat_heap_tuple_slot(&tys[0]))
    }

    /// The `(<scalar>, <flat heap>)` mirror of [`Self::is_flat_heap_scalar_pair_ty`].
    /// Extracted verbatim from [`Self::lower_owned_heap_field_aggregate`]'s arm guard
    /// (codopsy round-3 sweep, #852).
    fn is_scalar_flat_heap_pair_ty(&self, ty: &Ty) -> bool {
        matches!(ty,
            Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && is_heap_ty(&tys[1])
                && self.is_flat_heap_tuple_slot(&tys[1]))
    }
}

/// A `(String, <Fn>)` pair — String slot 0, closure block slot 1. Extracted verbatim from
/// [`LowerCtx::lower_owned_heap_field_aggregate`]'s arm guard (codopsy round-3 sweep, #852).
fn is_str_closure_pair_ty(ty: &Ty) -> bool {
    matches!(ty,
        Ty::Tuple(tys) if tys.len() == 2
            && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Fn { .. }))
}

/// A `(Value, <scalar>)` pair — the heap Value @12 needs the recursive `$__drop_value_tuple`,
/// not a flat per-slot dec. Extracted verbatim from
/// [`LowerCtx::lower_owned_heap_field_aggregate`]'s arm guard (codopsy round-3 sweep, #852).
fn is_value_scalar_pair_ty(ty: &Ty) -> bool {
    matches!(ty,
        Ty::Tuple(tys) if tys.len() == 2
            && crate::lower::is_value_ty(&tys[0]) && !is_heap_ty(&tys[1]))
}

impl LowerCtx {

    /// Calls, member access and the operator forms.
    ///
    /// One group of `lower_owned_heap_field`'s arm table, arms verbatim and in
    /// source order. The two negatives are DISTINCT and must stay so: `None`
    /// means "not my group", so the router tries the next one, while
    /// `Some(None)` is an arm DECLINING the field — what the single table's
    /// `_ => None` meant. Collapsing them makes a declined field fall through to
    /// a later group and lower by the wrong rule.
    fn lower_owned_heap_field_call(&mut self, expr: &IrExpr) -> Option<Option<ValueId>> {
        use almide_ir::BinOp;
        Some(match &expr.kind {
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
                    return Some(None);
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
            // An Option/Result-SUBJECT match reaches here whenever a variant combinator is
            // used as an owned-heap ELEMENT — `some(option.unwrap_or(s0, (1, 2)))` desugars
            // the inner call to `match s0 { some(x) => x, none => (1, 2) }`, and THAT match is
            // the OptionSome ctor's argument. `desugar_match_to_if` below only knows LITERAL
            // arm chains (int literals + a catch-all, or a 2-arm Bool), so a `some`/`none` arm
            // pair declined and the whole aggregate walled — the R3 fuzz finding. Every OTHER
            // match-lowering site (binds_p2, calls_p4, control_p3, tail_b,
            // heap_result_ctrl_arms) already tries the variant value-match ladder FIRST; this
            // one site did not, which is the entire defect. `try_lower_variant_value_match`
            // rolls its own ops/lifted/live-heap marks back on decline, so trying it costs
            // nothing when the shape is out of subset — the literal path below still runs.
            IrExprKind::Match { subject, arms } if crate::lower::is_variant_ty(&subject.ty) => {
                // The merged if-result `dst` is the ONE owned rc=1 value (whichever arm ran),
                // exactly like the `If` arm above — push it so the enclosing aggregate's
                // per-slot `Consume` MOVES it into the slot.
                let obj = self.try_lower_variant_value_match(subject, arms, &expr.ty)?;
                if !self.live_heap_handles.contains(&obj) {
                    self.live_heap_handles.push(obj);
                }
                Some(obj)
            }
            IrExprKind::Match { subject, arms } => {
                let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
                let IrExprKind::If { cond, then, else_ } = &if_expr.kind else {
                    return Some(None);
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
            // A CLOSURE-CALL field (`{ ...c, _val: f(c._val) }` — cell.update's
            // spread override, `f` a Fn param): dispatch through the tracked
            // closure block exactly like the bind-position Computed arm
            // (`lower_bind_heap_call_computed`) — the funcref returns a FRESH
            // owned heap value (the same cert `i` a Named CallFn result
            // carries), tracked so the enclosing aggregate's `Consume` (`m`)
            // moves it into the slot. An unresolvable callee container defers
            // (`None`) → the aggregate walls (never wrong bytes).
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } => {
                let blk = self.closure_block_of_mut(callee)?;
                let repr = repr_of(&expr.ty).ok()?;
                let lowered = self.lower_call_args(args).ok()?;
                let obj = self.fresh_value();
                self.emit_closure_call(blk, Some(obj), lowered, Some(repr));
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            // A BLOCK field — the heap-if-argument ANF (and its desugar
            // siblings) wraps a field value in a Block carrying its `let`s
            // (`{ ...c, _val: { let t = c._val; f(t) } }` — cell.update's
            // hoisted spread override, #881). Lower the statements as effects
            // in a field-local frame, then the TAIL as the field value itself
            // — the same frame discipline as the list Block element
            // (`lower_record_list_element`); the produced object's tracking
            // is restored so the enclosing aggregate's Consume still sees it.
            IrExprKind::Block { stmts, expr: tail } => {
                let tail = tail.as_deref()?;
                let mark = self.live_heap_handles.len();
                for s in stmts {
                    if let Err(e) = self.lower_stmt(s) {
                        crate::trace::trace("ALMIDE_DBG_ELEM", || {
                            format!("[field-block] stmt declined: {e:?}")
                        });
                        return Some(None);
                    }
                }
                let Some(obj) = self.lower_owned_heap_field(tail) else {
                    crate::trace::trace("ALMIDE_DBG_ELEM", || {
                        format!(
                            "[field-block] tail declined: {}",
                            crate::lower::kind_name(&tail.kind)
                        )
                    });
                    return Some(None);
                };
                self.live_heap_handles.retain(|h| *h != obj);
                self.drop_arm_locals(mark);
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            _ => return None,
        })
    }
}
