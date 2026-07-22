/// The element's drop kind for `try_lower_record_list_literal_as`'s list-literal builder: a
/// recursive-drop record (`$__drop_list_<R>`), a `(String,String)` tuple (`Op::DropListStrStr` —
/// the map.entries / `[(k,v), …]` literal shape), OR an Option/Result CTOR element (`[some(1),
/// none]`, `[ok(1), err("x")]` — the collect-test shapes): a Flat class (scalar payload — the
/// per-element `rc_dec` of `DropListStr` is exact) or a LenLoop class (owned handle slots — the
/// generated `$__drop_list_lenlist`). Moved to module scope (was function-local) so
/// `LowerCtx::classify_list_elem_drop` can return it — pure text move, no behavior change.
enum ListElemDrop {
    Record(String),
    StrStr,
    StrInt,
    IntStr,
    StrVariant(String),
    StrMapStr,
    StrListOpt,
    RecordInt(String),
    MapMlo,
    ListStr,
    MapHval,
    ScalarAggregate,
    CtorFlat,
    CtorLenLoop,
    Closure,
    StrClosure,
}

impl LowerCtx {
    /// Construct a SCALAR-field tuple `(a, b, …)`: alloc an n-slot block (Init::DynList) and store
    /// each field's computed scalar value at its slot via `Prim::Store`. Returns `None` (caller
    /// falls back to the Opaque alloc) if any field is heap or not a lowerable scalar.
    /// A scalar `List[Int]`/`List[Float]`/`List[Bool]` LITERAL with NON-literal elements (`[1.0, inf,
    /// 0.5]`, `[a, a]`, `[f(x), g(y)]`) — an all-literal list is already an `Init::IntList`, but a
    /// computed element can't be folded to a constant, so build the block explicitly: alloc `n` i64
    /// slots and `store64` each element's lowered scalar value (a Float's f64 bits, an Int's value).
    /// Scalar elements own no heap, so a flat `DynList` (drops as a flat block) is correct — no nested
    /// ownership. Returns None (defer to the Opaque alloc) if any element is heap or non-scalar-
    /// lowerable. The list-shaped sibling of [`Self::try_lower_scalar_tuple_construct`].
    pub(crate) fn try_lower_scalar_list_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else {
            return None;
        };
        // Only SCALAR-element lists (List[Int]/Float/Bool). A heap-element list is the str path above.
        let scalar_list = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
        if !scalar_list || elements.is_empty() {
            return None;
        }
        self.try_lower_scalar_list_slots(elements)
    }

    pub(crate) fn try_lower_scalar_tuple_construct(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        if elements.iter().any(|e| is_heap_ty(&e.ty)) {
            return None; // heap-element tuple → the masked `try_lower_tuple_construct` path.
        }
        let dst = self.try_lower_scalar_list_slots(elements)?;
        // A scalar tuple is built with the uniform slot layout, so `t.0` / a `${tuple}` Display
        // reads its real slots. No heap slots → only the SAFE scalar reads are enabled.
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Materialize a scalar-only tuple LITERAL element of a `List[(scalar, …)]` (`(1, 100)` in
    /// `[(1, 100), (127, 300)]`). Takes the tuple `IrExpr`, builds the fresh OWNED flat block, and
    /// returns its handle for the list-slot store. `None` (the list defers) on a non-tuple or a
    /// heap-field tuple element. The element does NOT join `materialized_aggregates` (the FOR-loop
    /// var binding tracks its own per-iteration handle); it is just the owned slot value moved in.
    fn try_lower_scalar_tuple_construct_for_elem(&mut self, elem: &IrExpr) -> Option<ValueId> {
        let IrExprKind::Tuple { elements } = &elem.kind else {
            return None;
        };
        self.try_lower_scalar_tuple_construct(elements)
    }

    /// Construct a TUPLE `(e0, e1, …)` with one or more HEAP ELEMENTS (a String/List/nested
    /// aggregate alongside scalars) — the positional analogue of [`Self::try_lower_record_construct`].
    /// Same `[rc][len][cap]` + uniform-i64-slot block; each heap element is a fresh OWNED handle
    /// MOVED into its slot (cert `m`), tracked in `record_masks` so the drop frees exactly the heap
    /// slots then the block (a masked `DropListStr`, cert = the single `d`). Returns `None` (defer)
    /// for an element value not lowerable to an owned heap handle / scalar — then the tuple falls
    /// back to Opaque and a `${tuple}` Display WALLS (never wrong bytes). SOUND by the SAME argument
    /// as the record path (each heap element `i…m`, the block `i…d` — the balanced List[String] shape).
    pub(crate) fn try_lower_tuple_construct(&mut self, elements: &[IrExpr]) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        if elements.is_empty() {
            return None;
        }
        let n = elements.len();
        let heap_slots: Vec<usize> =
            (0..n).filter(|&i| is_heap_ty(&elements[i].ty)).collect();
        if heap_slots.is_empty() {
            return None; // all-scalar → `try_lower_scalar_tuple_construct` owns it.
        }
        // Lower every element first (before the alloc), as (slot-value, is-heap).
        let mut slots: Vec<(ValueId, bool)> = Vec::with_capacity(n);
        for e in elements {
            if is_heap_ty(&e.ty) {
                let obj = self.lower_owned_heap_field(e)?;
                slots.push((obj, true));
            } else {
                let v = self.lower_scalar_value(e)?;
                slots.push((v, false));
            }
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (idx, (v, is_heap)) in slots.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(idx) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let store_val = if is_heap {
                let handle = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                handle
            } else {
                v
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, store_val],
            });
            if is_heap {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        self.record_masks.insert(dst, heap_slots);
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a SCALAR-only RECORD `R { f0: e0, f1: e1, … }`: alloc a block laid out
    /// by [`Self::aggregate_field_tys`] + [`layout::field_slots`] (per-field TIGHT-PACKED
    /// at width-aware offsets after the `[rc][len][cap]` header) and `Prim::Store` each
    /// field's computed scalar at its own (offset, width). Unlike
    /// [`Self::try_lower_scalar_list_slots`] (uniform 8-byte slots), this honours each
    /// field's DECLARED width (Int8 → width 1, Bool/Int32 → 4, Int/Float → 8), so a
    /// `{ b: Int8, n: Int }` round-trips through `r.b`/`r.n` byte-exactly.
    ///
    /// The field order + concrete widths come from the record's TYPE (resolved via the
    /// layout registry, substituting generic params with the instantiated args — so a
    /// `Box[Int]` field `value: T` is sized as `Int`, the #650 concern), NOT the literal's
    /// field order: construction and `r.x` projection consult the SAME declaration-ordered
    /// slot list, so they cannot desync even if the literal lists fields out of order.
    ///
    /// Returns `None` (defer/wall) for a non-record / unresolvable type, a HEAP field
    /// (needs an ownership-aware recursive drop — out of this value-model brick), an
    /// unsupported scalar width, or a field whose value is not a lowerable scalar.
    ///
    /// SOUNDNESS: a scalar-only record owns NO nested heap, so the block is a FLAT
    /// `DynList` — its scope-end drop is the ordinary single `Drop` (cert `i`+`d`), no
    /// new ownership op or certificate event. The fields are pure `Prim::Store`s (no
    /// ownership), exactly like the scalar-tuple / IntList path: one i64 slot per field,
    /// `12 + idx*8`, `store64`. A narrow Int8 value round-trips losslessly through its
    /// i64 slot, so a uniform slot is correct for the observable output.
    pub(crate) fn try_lower_scalar_record_construct(
        &mut self,
        value: &IrExpr,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        // Only an explicit `Record` literal reaches here (a `SpreadRecord` defers).
        let IrExprKind::Record { fields, .. } = &value.kind else {
            return None;
        };
        // A RECORD-CTOR literal is a TAGGED variant value — route to the variant builder
        // (see try_lower_record_construct's twin guard).
        if let IrExprKind::Record { name: Some(n), .. } = &value.kind {
            if self.variant_layouts.ctor_to_type.contains_key(n.as_str()) {
                return self.try_lower_variant_ctor(value);
            }
        }
        // The CANONICAL declaration-ordered (name, concrete-type) field list. A heap
        // field / unresolvable type ⇒ `None` (via `scalar_slots`) ⇒ wall.
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        let n = layout::scalar_slots(&tys)?;
        if names.len() != n {
            return None;
        }
        // Lower every supplied field value FIRST (before the alloc) so a field expr that
        // itself allocates does not interleave with our store sequence. Map each literal
        // field to its DECLARED index (the literal may list fields out of declaration
        // order — the slot offset follows the declaration, not the literal). A record may
        // OMIT a field (default) — the fresh block's slot stays zero, never garbage.
        let mut field_vals: Vec<(usize, ValueId)> = Vec::with_capacity(fields.len());
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            // A field whose VALUE is heap is out of the scalar value-model — wall the
            // whole record (never a partial wrong-bytes block).
            if is_heap_ty(&expr.ty) {
                return None;
            }
            let v = self.lower_scalar_value(expr)?;
            field_vals.push((idx, v));
        }
        // Rung-5 records slab: the block IS a scalar list (identical [rc][len][cap]
        // header + i64 slots), so the TARGET-NEUTRAL `Op::ListLit` builds it on both
        // legs — wasm renders the same alloc+stores as before (cert `i`, unchanged
        // stream), native renders `vec![…]`. Slots follow DECLARATION order; an
        // omitted (defaulted) field materializes the zero its slot previously kept.
        let _ = IntOp::Add; // (kept import shape; the prim sequence is gone)
        let _ = PrimKind::Handle;
        let mut slot_vals: Vec<ValueId> = Vec::with_capacity(n);
        for idx in 0..n {
            if let Some((_, v)) = field_vals.iter().find(|(i, _)| *i == idx) {
                slot_vals.push(*v);
            } else {
                let z = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: z, value: 0 });
                slot_vals.push(z);
            }
        }
        let dst = self.fresh_value();
        self.ops.push(Op::ListLit { dst, elems: slot_vals });
        // Built with the uniform slot layout, so a `${record}` Display (and a heap-field
        // borrow, were a later field heap) may read its real slots. A scalar-only record has
        // no heap slots, so this only enables the SAFE field reads — never a garbage deref.
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a custom-variant value `Ctor(args…)` (ADT brick 2) as the v1 value-model
    /// block: a `slot_count`-wide uniform-i64-slot block — the SAME `[rc][len][cap]` header +
    /// i64-slot layout a record uses (NOT v0's byte-packed `[tag][packed fields]`; only the
    /// OBSERVABLE output byte-matches v0, never the internal bytes) — whose slot 0 holds the
    /// constructor's TAG and slots `1+i` hold its i-th field. SCALAR fields only: a
    /// heap/recursive ctor field (a nested variant, a `String`) is an ADT-brick-5 concern, so
    /// `None` (the caller walls — never a partial wrong-bytes block). The block is one owned
    /// allocation (cert `i`; its scope-end `Drop` = cert `d`), tracked as a materialized
    /// aggregate so a later field read / `==` may load its real slots. Mirrors
    /// [`Self::try_lower_scalar_record_construct`] with a leading tag slot.
    /// Is `ty` a `List` ctor field the GENERATED variant drop can free — a `List[scalar]`
    /// (the drop body's flat `rc_dec` is a full free: scalar elements own nothing), a
    /// `List[String]` (freed per-element via the generic `__drop_list_str`), or a
    /// `List[<rich variant>]` (freed per-element via the generated mutually-recursive
    /// `$__drop_list_<E>`)? The construction-side mirror of the field loop in
    /// [`crate::lower::generate_variant_drop_sources`] — a shape outside this set
    /// (`List[<flat variant>]`, `Map`) gets NO free statement there, so admitting it here
    /// would build a value whose drop leaks.
    fn ctor_list_field_drop_freeable(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::List, a) = ty else { return false };
        if a.len() != 1 {
            return false;
        }
        if !is_heap_ty(&a[0]) || matches!(a[0], Ty::String) {
            return true;
        }
        // A rich (recursive-drop) variant element frees via the generated `$__drop_list_<E>`;
        // a FLAT variant element (nullary/scalar-only ctors — `Wrapped(List[Policy])`, #484)
        // frees via the generated `$__drop_<T>`'s `__drop_list_str` per-element sweep (the
        // List[flat-variant] case, mirroring the record generator's precedent).
        self.variant_layouts
            .field_variant_name(&a[0])
            .is_some_and(|n| self.variant_layouts.needs_recursive_drop(&n, &|_| false))
            || self.variant_layouts.is_flat_variant_ty(&a[0])
    }

    /// Is `ty` a scalar, OR a ONE-LEVEL-EXACT heap type — a value whose ENTIRE free is a
    /// single `rc_dec` (it owns no further heap): `String`, `List[scalar]`, a FLAT record
    /// (every field scalar — `record_or_anon_drop_type_name` already excludes it from the
    /// RECURSIVE-drop set, so reaching here at all means flat), or a flat variant (every
    /// ctor scalar-only, `is_flat_variant_ty`). Gates the list-literal tuple-pair
    /// classifier (`StrStr`/`StrInt`/`IntStr`) to shapes `Op::DropListStrStr`/
    /// `DropListStrInt`/`DropListIntStr`'s PURELY HANDLE-BASED renders (confirmed by
    /// reading their WAT/self-host bodies: each just `rc_dec`s a raw slot handle, no
    /// byte/length interpretation) free EXACTLY — a NESTED-heap type (`List[String]`, a
    /// RECURSIVE-drop record, `Value`) would leak under a blind single `rc_dec`, the same
    /// class of bug this session's `_str`-dispatch fix caught elsewhere.
    fn is_flat_heap_tuple_slot(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(ty) {
            return true; // a scalar needs no free at all — vacuously "flat"
        }
        matches!(ty, Ty::String)
            || matches!(ty, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]))
            || self.variant_layouts.is_flat_variant_ty(ty)
            || (self.record_or_anon_drop_type_name(ty).is_none()
                && self
                    .aggregate_field_tys(ty)
                    .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t))))
    }

    pub(crate) fn try_lower_variant_ctor(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        // The ctor NAME + its supplied field exprs in DECLARED case order — from a
        // positional ctor CALL (`IntV(p)`) or a RECORD-ctor literal (`Data { payload: …,
        // seq: … }`, whose IR is a NAMED Record; field order follows the case, and a
        // missing field walls — a defaulted variant-record slot would be garbage).
        let (ctor_name, args): (String, Vec<IrExpr>) = match &value.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                (name.as_str().to_string(), args.clone())
            }
            IrExprKind::Record { name: Some(ctor), fields }
                if self.variant_layouts.ctor_to_type.contains_key(ctor.as_str()) =>
            {
                let ctor_s = ctor.as_str().to_string();
                let case_fields = {
                    let (_, _, case) = self.variant_layouts.lookup_ctor(&ctor_s)?;
                    case.fields.clone()
                };
                let mut ordered = Vec::with_capacity(case_fields.len());
                for (fname, _) in &case_fields {
                    let e = match fields.iter().find(|(n, _)| n == fname) {
                        Some((_, e)) => e.clone(),
                        // An OMITTED defaulted field (`Rect { width, height }` with
                        // `color = ""`): fill the DECLARED default expr, evaluated at
                        // construction exactly as v0 does. Gated CALL-FREE (a call-bearing
                        // default would add a MIR call the counted IR lacks — mir>ir);
                        // the corpus defaults are literals (`""`, `false`, `[]`).
                        Option::None => {
                            let d = self
                                .variant_layouts
                                .ctor_field_defaults
                                .get(&ctor_s)
                                .and_then(|m| m.get(fname.as_str()))?;
                            if crate::lower::expr_contains_call(d) {
                                return Option::None;
                            }
                            d.clone()
                        }
                    };
                    ordered.push(e);
                }
                (ctor_s, ordered)
            }
            _ => return None,
        };
        // Resolve the ctor's tag + the type's uniform block width + the OWNING TYPE NAME from the
        // registry. Cloned out of the immutable borrow so the lowering below can mutate `self`.
        let (tag, slot_count, arity, type_name) = {
            let (ty, layout, case) = self.variant_layouts.lookup_ctor(&ctor_name)?;
            (case.tag as i64, layout.slot_count, case.fields.len(), ty.to_string())
        };
        if args.len() != arity {
            return None;
        }
        // Does this TYPE need the recursive DropVariant (a nested-variant OR nested-record field)? If
        // so, its heap fields are freed by the generated `$__drop_<T>`, NOT the masked DropListStr.
        // The record predicate mirrors the drop generator's `variant_needs_recursive_drop` widening.
        let needs_rec = self
            .variant_layouts
            .needs_recursive_drop(&type_name, &|rn| {
                crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
            });
        // Lower every field value FIRST (before the alloc) so a field expr that itself allocates
        // does not interleave with our store sequence. A SCALAR field is a value copy; a leaf
        // `String` field is a fresh OWNED handle (lower_owned_heap_field) moved in; a NESTED
        // VARIANT field is recursively constructed (a ctor call → try_lower_variant_ctor) or
        // `Dup`'d (a var → lower_owned_heap_field) and moved in — its recursive free is the
        // generated `$__drop_<T>`. A List/other heap field is still ADT-brick-5+ → WALL.
        let mut field_vals: Vec<(ValueId, bool /* is_heap */)> = Vec::with_capacity(args.len());
        for arg in &args {
            if self.variant_layouts.field_is_variant(&arg.ty) {
                // A nested ctor field — positional (`Leaf(1)`) OR a record-ctor literal
                // (`right: Node { … }`) — recurses into this same builder.
                let is_ctor_call = matches!(
                    &arg.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.variant_layouts.ctor_to_type.contains_key(name.as_str())
                ) || matches!(
                    &arg.kind,
                    IrExprKind::Record { name: Some(n), .. }
                        if self.variant_layouts.ctor_to_type.contains_key(n.as_str())
                );
                let v = if is_ctor_call {
                    self.try_lower_variant_ctor(arg)?
                } else {
                    self.lower_owned_heap_field(arg)?
                };
                field_vals.push((v, true));
                continue;
            }
            if matches!(arg.ty, Ty::String) {
                let obj = self.lower_owned_heap_field(arg)?;
                field_vals.push((obj, true));
                continue;
            }
            if self.ctor_list_field_drop_freeable(&arg.ty) {
                // A `List[scalar]` / `List[<rich variant>]` ctor field (ADT brick 5:
                // `ValArray(items)` — the gguf read_array accumulator): admitted EXACTLY when
                // the generated `$__drop_<T>` body frees it (flat `rc_dec` / `__drop_list_<E>`
                // — see `generate_variant_drop_sources`' field loop), so construction and drop
                // can never disagree. A Var arg is `Dup`'d (co-owned, rc-aware on both drop
                // paths); a `List[String]` / `List[<flat variant>]` / `Map` field stays walled
                // (the generator emits no free for those — admitting one would leak).
                let obj = self.lower_owned_heap_field(arg)?;
                field_vals.push((obj, true));
                continue;
            }
            if matches!(&arg.ty, Ty::Named(..) | Ty::Record { .. })
                && self.aggregate_field_tys(&arg.ty).is_some()
            {
                // A RECORD-type ctor field (`Wrap(Color)`, `Box(Inner)`): materialize the record (a
                // `Record` literal via `try_lower_record_construct` / the scalar builder; a decoded Var /
                // call via `lower_owned_heap_field`) and store its handle. Because the variant now counts
                // a record field in `needs_recursive_drop`, its scope-end drop is the generated
                // `$__drop_<V>` — which frees the field via `$__drop_<R>` (a nested-heap record) or a flat
                // `rc_dec` (a scalar-only record), so the record's nested heap is never leaked.
                let obj = match &arg.kind {
                    IrExprKind::Record { .. } => self
                        .try_lower_record_construct(arg)
                        .or_else(|| self.try_lower_scalar_record_construct(arg))?,
                    _ => self.lower_owned_heap_field(arg)?,
                };
                field_vals.push((obj, true));
                continue;
            }
            if matches!(&arg.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
            {
                // An Option[scalar] ctor field (`Box(Some(8))`, `Box(None)`): the 0-or-1-element
                // len-tag block owns NO children, so its free is one flat rc_dec — emitted by the
                // generated `$__drop_<T>` (the Option arm in the drop generator's field loop; the
                // widened `needs_recursive_drop` makes this type recursive-drop) or the masked
                // DropListStr. A ctor expr builds the fresh block (`try_lower_option_ctor`); a
                // Var is Dup'd/moved via `lower_owned_heap_field`. Option[heap] / Result payloads
                // own children a flat free would leak — they stay walled (a later brick).
                let obj = self
                    .try_lower_option_ctor(arg, &arg.ty)
                    .or_else(|| self.lower_owned_heap_field(arg))?;
                field_vals.push((obj, true));
                continue;
            }
            if matches!(&arg.ty, Ty::Fn { .. }) {
                // A CLOSURE ctor field (`Run(() => …)` / `Thunk((x) => x * x)` — the
                // variant-stored closure class): a Lambda arg LIFTS to its closure
                // block, a Var arg Dups the tracked block (both via
                // `lower_owned_heap_field`'s existing arms); the ctor then owns the
                // block and the generated `$__drop_<T>`'s Fn arm frees it via
                // `__drop_closure` (the classifier + generator admit Fn fields in
                // the same change — construction and drop agree).
                let obj = self.lower_owned_heap_field(arg)?;
                field_vals.push((obj, true));
                continue;
            }
            if is_heap_ty(&arg.ty) {
                return None; // List[String] / Map / other heap ctor field — a later brick
            }
            let v = self.lower_scalar_value(arg)?;
            field_vals.push((v, false));
        }
        // Rung-5 variants slab: an ALL-SCALAR ctor block is a plain slot list
        // (tag@slot0, fields@1+, zero-filled to the type's uniform width), so
        // the TARGET-NEUTRAL `Op::ListLit` builds it on both legs — same cert
        // `i`, same block bytes. Heap-field ctors keep the prim path below.
        if field_vals.iter().all(|(_, is_heap)| !is_heap) {
            let tagv = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: tagv, value: tag });
            let mut slot_vals: Vec<ValueId> = Vec::with_capacity(slot_count);
            slot_vals.push(tagv);
            for (v, _) in &field_vals {
                slot_vals.push(*v);
            }
            while slot_vals.len() < slot_count {
                let z = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: z, value: 0 });
                slot_vals.push(z);
            }
            let dst = self.fresh_value();
            self.ops.push(Op::ListLit { dst, elems: slot_vals });
            // EXACT tracking mirror of the prim path below (heap_slots is empty
            // here, so only the needs_rec branch and the aggregate mark apply).
            if needs_rec {
                self.variant_drop_handles.insert(dst, type_name);
            }
            self.materialized_aggregates.insert(dst);
            return Some(dst);
        }
        // Allocate the `slot_count`-wide block.
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: slot_count as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        // Store the tag into slot 0, then each field into slot `1+i`. A heap field stores its
        // HANDLE (i64-widened) then is `Consume`d (moved in); a scalar field stores its value.
        let tagv = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tagv, value: tag });
        let store_addr = |s: &mut Self, slot: usize| {
            let off = s.fresh_value();
            s.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(slot) as i64 });
            let addr = s.fresh_value();
            s.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            addr
        };
        let addr0 = store_addr(self, 0);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr0, tagv] });
        let mut heap_slots: Vec<usize> = Vec::new();
        for (i, (v, is_heap)) in field_vals.into_iter().enumerate() {
            let slot = 1 + i;
            let addr = store_addr(self, slot);
            let store_val = if is_heap {
                let handle = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                handle
            } else {
                v
            };
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, store_val] });
            if is_heap {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
                heap_slots.push(slot);
            }
        }
        // Drop selection: a NESTED-variant type uses the recursive `Op::DropVariant` (the
        // generated `$__drop_<T>` frees every heap field — variant slots recursively, String
        // slots flat — then the block). A String-only-field type uses the masked DropListStr.
        if needs_rec {
            self.variant_drop_handles.insert(dst, type_name);
        } else if !heap_slots.is_empty() {
            self.record_masks.insert(dst, heap_slots);
        }
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// Construct a record/tuple with one or more HEAP FIELDS (a `String`/`List`/nested
    /// aggregate field alongside scalar fields) — `R { name: "x", n: i }`. The block is the
    /// SAME `[rc][len][cap]` + uniform-i64-slot layout as the scalar path, but each HEAP
    /// field is a fresh OWNED handle MOVED into its slot (cert `m`), and the value is tracked
    /// in `record_masks` so its drop frees exactly the heap slots then the block (an
    /// [`Op::DropListStr`] with the per-value mask — cert = the SAME single `d`).
    ///
    /// SOUNDNESS (no new op / no certificate change): this is byte-identical to the
    /// `List[String]` machinery applied to a mixed slot set. A heap field's owned handle is
    /// `Consume`d into the slot (cert `m` — moved in, like `prim.store_str`), so each heap
    /// field is `i…m` (alloc/dup then move-in) and the BLOCK is `i…d` (alloc then the
    /// recursive `DropListStr`), exactly the balanced shape the proven checker already
    /// accepts for a list of Strings. A scalar field is a pure `Prim::Store` (no ownership).
    /// The recursive free at drop touches ONLY the heap slots (the mask) — a scalar slot is
    /// never `rc_dec`'d. Returns `None` (defer) for an unresolvable type, an omitted heap
    /// field (a defaulted heap slot would be a garbage handle the drop frees — unsound), or
    /// a field value not lowerable to an owned handle / scalar.
    pub(crate) fn try_lower_record_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::Record { fields, .. } = &value.kind else {
            return None;
        };
        // A RECORD-CTOR literal (`Data { payload: …, seq: … }` — the NAME is a registered
        // variant constructor): this is a TAGGED variant value, NOT a plain record — route
        // to the variant builder (a tag-less field block here would misread every match).
        if let IrExprKind::Record { name: Some(n), .. } = &value.kind {
            if self.variant_layouts.ctor_to_type.contains_key(n.as_str()) {
                return self.try_lower_variant_ctor(value);
            }
        }
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        if tys.is_empty() {
            return None;
        }
        // DEFAULT FILL: an omitted slot with a DECLARED default (`type AllDefault = {
        // host: String = "localhost", port: Int = 8080 }`; `AllDefault()`) synthesizes
        // the default as a supplied field — CALL-FREE defaults only (a call default
        // would inject an uncounted CallFn, breaching the caps mir == ir gate; it
        // keeps walling via the omitted-heap check below).
        let mut fields = fields.clone();
        if let Ty::Named(rec_name, _) = &value.ty {
            if let Some(defs) = self
                .variant_layouts
                .ctor_field_defaults
                .get(rec_name.as_str())
                .cloned()
            {
                for nm in &names {
                    if fields.iter().any(|(fname, _)| fname == nm) {
                        continue;
                    }
                    if let Some(d) = defs.get(nm.as_str()) {
                        if !crate::lower::expr_contains_call(d) {
                            fields.push((*nm, d.clone()));
                        }
                    }
                }
            }
        }
        let fields = &fields;
        let n = tys.len();
        // Per-slot heap-ness from the SUPPLIED field's CONCRETE type (`expr.ty`), NOT the
        // declared field type — a generic field (`first: A` in `Pair[A,B]`) may leave the
        // DECLARED type an unresolved param that `is_heap_ty` would mis-classify as heap; the
        // literal's value carries the concrete instantiated type. `None` for an unsupplied
        // (defaulted) slot — its concrete heap-ness is unknown here.
        let mut field_heap: Vec<Option<bool>> = vec![None; n];
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            field_heap[idx] = Some(is_heap_ty(&expr.ty));
        }
        // A DEFAULTED (omitted) slot whose DECLARED type is concretely heap (or an unresolved
        // generic we can't prove scalar) would leave a zero handle the masked drop frees — so
        // WALL the whole record (never an unsound partial block). A scalar default (a 0 slot)
        // is fine. (An omitted scalar slot's `field_heap` stays `None` = treated non-heap.)
        for i in 0..n {
            if field_heap[i].is_none() && is_heap_ty(&tys[i]) {
                return None;
            }
        }
        let heap_slots: Vec<usize> =
            (0..n).filter(|&i| field_heap[i] == Some(true)).collect();
        if heap_slots.is_empty() {
            return None; // no heap field — `try_lower_scalar_record_construct` owns it.
        }
        // Lower each supplied field to (declared-index, slot-value, is-heap). Heap fields
        // become a fresh OWNED handle (the same kinds `try_lower_str_list_literal` admits);
        // scalar fields a plain value. All lowered BEFORE the alloc (a field expr that
        // itself allocates must not interleave with our store sequence).
        let mut slots: Vec<(usize, ValueId, bool)> = Vec::with_capacity(fields.len());
        for (name, expr) in fields {
            let idx = names.iter().position(|n| n == name)?;
            let is_heap = is_heap_ty(&expr.ty);
            if is_heap {
                let obj = self.lower_owned_heap_field(expr)?;
                slots.push((idx, obj, true));
            } else {
                let v = self.lower_scalar_value(expr)?;
                slots.push((idx, v, false));
            }
        }
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        for (idx, v, is_heap) in slots {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(idx) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // A heap field stores its HANDLE (i64-widened) then is `Consume`d (moved in);
            // a scalar field stores its value directly.
            let store_val = if is_heap {
                let handle = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                handle
            } else {
                v
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, store_val],
            });
            if is_heap {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        self.record_masks.insert(dst, heap_slots);
        self.materialized_aggregates.insert(dst);
        if let Some(name) = self.record_drop_type_name(&value.ty) {
            self.variant_drop_handles.insert(dst, name);
        }
        Some(dst)
    }

    /// Materialize a `List[Record]` LITERAL (`group([rect(…), circle(…)])`, `[el("a"), el("b")]`) — a
    /// list block whose i64 slots each hold an OWNED Element record handle (lowered via
    /// `lower_owned_heap_field`, MOVED in). Tracked so its scope-end drop routes to the generated
    /// `$__drop_list_<R>` (each element freed recursively via `$__drop_<R>`). GATE: the element type
    /// must be a record needing the recursive drop (`record_drop_type_name` Some), so `$__drop_list_<R>`
    /// exists; otherwise `None` (the caller keeps the scalar / wall path). Empty lists handled elsewhere.
    pub(crate) fn try_lower_record_list_literal(&mut self, value: &IrExpr) -> Option<ValueId> {
        self.try_lower_record_list_literal_as(value, None)
    }
}
