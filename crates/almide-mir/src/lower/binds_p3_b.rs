impl LowerCtx {

    /// The element-type → drop-shape classification for `try_lower_record_list_literal_as`'s
    /// list-literal builder — see [`ListElemDrop`]. Read-only over `self` (only `&self`
    /// lookups). A `return None` inside an arm below (the `StrVariant`/`RecordInt` cases)
    /// returns `None` from THIS helper, not from the caller directly; the call site's
    /// `let Some(kind) = self.classify_list_elem_drop(…) else { return None }` re-propagates
    /// it, so the caller's observable behavior is byte-for-byte unchanged from the former
    /// inline if-else-if chain. Verbatim extraction (guard-clause flattening), no behavior
    /// change — see docs/roadmap/active/code-health-codopsy.md.
    fn classify_list_elem_drop(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        self.classify_elem_drop_heads(elem_ty)
            .or_else(|| self.classify_elem_drop_pairs(elem_ty))
            .or_else(|| self.classify_elem_drop_containers(elem_ty))
    }

    /// Rung 1 of the element-drop ladder: record / variant element heads.
    /// (The three rungs are CONSECUTIVE slices of one ordered rule ladder —
    /// several arms depend on earlier arms having declined, so the order
    /// inside AND across rungs is load-bearing.)
    fn classify_elem_drop_heads(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        // A STRUCTURAL record element (`[{key: "x", val: "2"}]` in argument position —
        // the checker leaves the literal structural, so `record_drop_type_name` alone
        // declined it, calls_p2's List-arg wall): the synthesized anon-record drop
        // (`__drop_anonrec_<hash>`) covers it with the SAME field order the literal
        // materializes in — no declared-vs-structural order mismatch (the soundness
        // crux the named path guards).
        if let Some(rname) = self.record_or_anon_drop_type_name(elem_ty) {
            return Some(ListElemDrop::Record(rname));
        }
        // A PLAIN variant element (`[shapes.circle(1.0)]` — a `List[Figure]`
        // literal over a (cross-module) variant type, #875). A RICH variant
        // (heap-bearing payloads) routes to the generated per-element
        // recursive `$__drop_list_<V>` — the same `list_<name>` key the
        // Record registration uses, and `is_rich_variant_ty` asks the SAME
        // question the drop generator does, so admission ⊆ generation. A
        // FLAT one (all-scalar payloads — `Circle(Float)`) has no generated
        // list drop and needs none: per-element `rc_dec` frees each block
        // exactly (the CtorFlat class). Checked before the tuple arms (a
        // variant is never a tuple).
        if is_heap_ty(elem_ty) && !matches!(elem_ty, Ty::Tuple(_)) {
            let rich = self.variant_layouts.is_rich_variant_ty(elem_ty, &|rn| {
                crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
            });
            if let Some(vname) = rich {
                return Some(ListElemDrop::Record(vname));
            }
            if self.custom_variant_type_name(elem_ty).is_some() {
                return Some(ListElemDrop::CtorFlat);
            }
        }
        None
    }

    /// Rung 2: the 2-tuple pair shapes (order preserved — StrMapStr and
    /// StrClosure are checked before the generic StrVariant arm by design).
    fn classify_elem_drop_pairs(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        if matches!(elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
                && (matches!(tys[1], Ty::String)
                    || matches!(&tys[1],
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                            if b.len() == 1 && !is_heap_ty(&b[0]))))
        {
            // Widened to (String, <flat block>): DropListStrStr's per-tuple BOTH-slot
            // rc_dec is a full free for a String OR List[scalar] second slot — the hval
            // map literal's `("xs", [1, 2, 3])` pairs (the OWNED-builder route the PCC
            // ownership gate accepts, unlike the raw-handle view widening it rejected).
            return Some(ListElemDrop::StrStr);
        }
        if matches!(elem_ty, Ty::Tuple(tys)
            if tys.len() == 2
                && is_heap_ty(&tys[0])
                && is_heap_ty(&tys[1])
                && self.is_flat_heap_tuple_slot(&tys[0])
                && self.is_flat_heap_tuple_slot(&tys[1]))
        {
            // A `(<flat record/variant>, String)` TUPLE element (`[Color{r,g,b}: "red"]` —
            // the `[key: value]` map-literal desugar over a user Hash-key type,
            // hash_protocol_test's Color/Direction shapes): `Op::DropListStrStr`'s render
            // (`__ssdrop_list` in value_core.almd) is PURELY handle-based — `rc_dec` of the
            // raw handle at slot0 (@12) and slot1 (@20), reading NEITHER slot's internal
            // bytes — so it is exact for ANY pair of ONE-LEVEL-EXACT heap values, not just
            // two Strings (confirmed by reading its body: no `__str_eq`-style length/byte
            // interpretation, the exact class of bug this session's `_str`-dispatch fix
            // caught elsewhere). A FLAT record (`record_or_anon_drop_type_name` already
            // returned `None` above — only a RECURSIVE-drop record reaches that arm; an
            // all-scalar record like `Color` falls through to here) or a flat variant
            // (`Direction`, all-nullary) is exactly one-level-exact: a single `rc_dec`
            // frees the whole block, since it owns no further heap.
            return Some(ListElemDrop::StrStr);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && self.is_flat_heap_tuple_slot(&tys[0]) && is_heap_ty(&tys[0]) && !is_heap_ty(&tys[1]))
        {
            // A `(<flat heap>, <scalar>)` TUPLE element (`[("k0", 1), ("k1", 2)]` — the
            // `[key: value]` map-literal desugar's pairs list, map_fold_heap_acc's initial
            // accumulator, `[("k0", true), …]` — option_unwrap_or_else_heap's Map[String,
            // Bool]; `[East: 90, …]` — hash_protocol_test's `Map[Direction, Int]`): the
            // MIRROR of the IntStr arm below. Recursive drop via the EXISTING
            // `Op::DropListStrInt` (rc_dec slot0 @12 only — the render NEVER reads slot1's
            // contents, so it is scalar-type-agnostic: Int/Bool/Float all free identically;
            // and slot0-type-agnostic too, since it just rc_decs the raw handle — a flat
            // record/variant frees exactly like a String there) — the same Op
            // calls_p2.rs's concat-operator path already routes to for the (String,scalar)
            // instance, just not previously wired to the list-LITERAL classifier nor
            // widened past String. Was Int-only (B34), then any-scalar-value (B37); now
            // any-flat-heap-key too.
            return Some(ListElemDrop::StrInt);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && self.is_flat_heap_tuple_slot(&tys[1]) && is_heap_ty(&tys[1]))
        {
            // A `(<scalar>, <flat heap>)` TUPLE element (`[(0, "a"), (1, "b")]` —
            // `list.enumerate` shaped literals): recursive drop via the existing
            // `Op::DropListIntStr` (rc_dec slot1 @20 only — likewise type-agnostic).
            return Some(ListElemDrop::IntStr);
        }
        self.classify_elem_drop_str_keyed_pairs(elem_ty)
    }

    /// Rung 2b: the `(String, <container/closure/variant>)` pair shapes —
    /// the ordered continuation of rung 2 (same ladder, same order).
    fn classify_elem_drop_str_keyed_pairs(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        let Ty::Tuple(tys) = elem_ty else { return None };
        let [k, v] = &tys[..] else { return None };
        if matches!(k, Ty::String) {
            return self.str_keyed_pair_drop(v);
        }
        // A `(<RECURSIVE-DROP record>, <scalar>)` TUPLE element (`[({name: "alice", age:
        // 30}, 1), …]` — compound_eq's `Map[P, Int]` from_list pairs): the RECORD mirror
        // of `StrVariant`. `DropListStrInt` only rc_decs slot0 one level — P owns a String
        // field, so a flat rc_dec LEAKS it; slot0 must recurse via `$__drop_<R>`. The
        // element's record slot is FORCED to this declared/classified type at construction
        // (below), so classification name, construction layout, and the generated
        // `$__drop_list_<R>_int` teardown all key on ONE name — the mismatch that produced
        // the earlier attempt's dangling `$__drop_list_anonrec_<hash>_int`.
        if is_heap_ty(v) {
            return None;
        }
        Some(ListElemDrop::RecordInt(self.record_or_anon_drop_type_name(k)?))
    }

    /// The drop a `(String, V)` pair element needs, by its VALUE half `v`.
    /// Ordered most-specific first: every arm below is checked BEFORE the generic
    /// `StrVariant` one, whose variant-name lookup would DECLINE a Map/Fn slot and
    /// kill the whole builder.
    fn str_keyed_pair_drop(&self, v: &Ty) -> Option<ListElemDrop> {
        use almide_lang::types::constructor::TypeConstructorId;
        // A `Map[String, String]` value (the map_fold_heap_acc nested-map literal's
        // pairs list, `["k0": ["k0": "x"]]` desugared to `map.from_list_msv([("k0",
        // <inner map>)])`): slot1 is a MAP owning its own String slots — the static
        // `$__drop_list_str_mss` (map_msv.almd) frees slot0 flat and sweeps the
        // last-ref inner map (a flat rc_dec would leak every inner key/value String).
        if matches!(v, Ty::Applied(TypeConstructorId::Map, b)
            if b.len() == 2 && matches!(b[0], Ty::String) && matches!(b[1], Ty::String))
        {
            return Some(ListElemDrop::StrMapStr);
        }
        // A `Map[String, <scalar>]` / `Option[String]` / len-counted value: all follow
        // the len@4-counted String-slot discipline (see `is_map_msb_ty`), so slot1 owns
        // exactly its len-counted String slots — the static `$__drop_list_str_msb`
        // (map_msv.almd) frees slot0 flat and len-sweeps the last-ref value block.
        if is_msb_pair_value(v) {
            return Some(ListElemDrop::StrMapSkv);
        }
        // A `List[Option[Int]]` value (compound_repr_interp's `deep` pairs list,
        // `["k": [some(1), none]]` desugared to `map.from_list_mlo([("k", <lenlist>)])`):
        // slot1 is a LIST owning its Option-block slots — the static
        // `$__drop_list_str_mlo` (map_mlo.almd) frees slot0 flat and sweeps the last-ref
        // inner list (a flat rc_dec would leak every Option block).
        if matches!(v, Ty::Applied(TypeConstructorId::List, b)
            if b.len() == 1
                && matches!(&b[0], Ty::Applied(TypeConstructorId::Option, o)
                    if o.len() == 1 && matches!(o[0], Ty::Int)))
        {
            return Some(ListElemDrop::StrListOpt);
        }
        // A `<Fn>` value (`map.from_list([("a", () => …)])` — the closure-valued map's
        // pairs list): slot1 is a CLOSURE BLOCK whose captured env a flat rc_dec would
        // leak — the static `$__drop_list_str_clo` frees slot0 flat (String rc_dec) and
        // routes slot1 through `__drop_closure`.
        if matches!(v, Ty::Fn { .. }) {
            return Some(ListElemDrop::StrClosure);
        }
        // A RICH variant value (`[("x", ValInt(64)), ("y", ValStr("s"))]` —
        // generic_chain_unwrap_or's `List[(String, V)]` metadata pairs,
        // `type V = ValInt(Int) | ValStr(String)`): the MIRROR of `StrInt`, but slot1 is
        // NOT scalar — it is a variant needing its OWN recursive drop (a `ValStr` payload
        // owns a String). `DropListStrInt`'s render only ever rc_decs slot0 and leaves
        // slot1 UNTOUCHED (sound only when slot1 is truly scalar) — reusing it here would
        // silently LEAK every `ValStr` element's String, so this is a genuinely new drop
        // shape: a generated `$__drop_list_str_<V>` (drop_sources.rs) frees slot0 (String,
        // flat rc_dec) AND recurses into slot1 via the variant's own already-generated
        // `$__drop_<V>` (V is a real, non-generic type — no shadow-type machinery needed,
        // unlike B117's generic-instantiation case).
        if !is_heap_ty(v) || self.is_flat_heap_tuple_slot(v) {
            return None;
        }
        Some(ListElemDrop::StrVariant(self.custom_variant_type_name(v)?))
    }

    /// Rung 3: the container elements (List/Map/Option families and the
    /// all-scalar aggregate tail).
    fn classify_elem_drop_containers(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        if matches!(elem_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, i)
                if i.len() == 1 && matches!(i[0], Ty::String))
        {
            // A `List[List[String]]` literal (`[["b","2"], ["a","1"]]` — the sort_by
            // string-key shape): each inner list is a fresh owned DynListStr; the outer
            // drop is the recursive list-of-list-str free (`list_list_str_lists`).
            return Some(ListElemDrop::ListStr);
        }
        if matches!(elem_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, kv)
                if kv.len() == 2 && matches!(kv[0], Ty::String)
                    && matches!(&kv[1],
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                            if b.len() == 1 && matches!(b[0], Ty::Int)))
        {
            // A `List[Map[String, List[Int]]]` literal (`[["a": [1, 2]], ["b": [3]]]` —
            // the nested repr shape): each element is an hval map block (a from_list_hval
            // call result, moved in); the list frees per-element via the self-hosted
            // `$__drop_list_map_hval` (each element through `__drop_map_hval`).
            return Some(ListElemDrop::MapHval);
        }
        if crate::lower::is_map_mlo_ty(elem_ty) {
            // A `List[Map[String, List[Option[Int]]]]` literal (compound_repr_interp's
            // `deep` outer list): each element is an mlo map block (a from_list_mlo call
            // result, moved in); the list frees per-element via the self-hosted
            // `$__drop_list_map_mlo` (each element through `__drop_map_mlo`).
            return Some(ListElemDrop::MapMlo);
        }
        if matches!(elem_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                if b.len() == 1 && !is_heap_ty(&b[0]))
        {
            // A `List[List[<scalar>]]` literal ARG (`[[1, 2], [3, 4]]` — compound_eq's
            // lnl): each inner list is a fresh FLAT block (inline scalars), so the
            // per-element rc_dec of the masked DropListStr is its full free — the same
            // ScalarAggregate physics with a list-literal element materializer.
            return Some(ListElemDrop::ScalarAggregate);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t)))
        {
            // An ALL-SCALAR tuple element (`[(1, 2), (3, 4)]` — the compound_eq
            // List[(Int, Int)] argument): each element is a fresh flat block (inline
            // scalars only), so the per-element rc_dec of the masked DropListStr IS its
            // full free. The OWNED route (build + Consume) — the raw-handle view trap
            // (B24) double-frees this shape.
            return Some(ListElemDrop::ScalarAggregate);
        }
        self.classify_elem_drop_map_options(elem_ty)
    }

    /// Rung 3b: the Option/Map element families and the flat-aggregate tail
    /// — the ordered continuation of rung 3 (same ladder, same order).
    fn classify_elem_drop_map_options(&self, elem_ty: &Ty) -> Option<ListElemDrop> {
        // An `Option[Map[String, <scalar>]]` element (`[some(["k0": true]), some(n1),
        // none]` — Wave 4 L6): the payload map breaks the lenlist "one-level-exact"
        // rule (its interior owns key Strings), so it takes its OWN class with the
        // static 3-level `$__drop_list_omb` (list → option block len-slot → the msb
        // key sweep) instead of widening the shared `$__drop_list_lenlist` in place.
        // Decided BEFORE the lenlist arm on purpose — lenlist_elem_class would
        // return None for a map payload and fall to the nested-ownership wall.
        if matches!(elem_ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, o)
            if o.len() == 1
                && matches!(&o[0], Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, b)
                    if b.len() == 2 && matches!(b[0], Ty::String)
                        && matches!(b[1], Ty::Bool | Ty::Int | Ty::Float)))
        {
            return Some(ListElemDrop::OptMapSkv);
        }
        // A BARE `Map[String, <scalar>]` element (`[["k0": true], ["k1": false]]` —
        // Wave 4 P2, reduced to `let xs: List[Map[String, Bool]] = [["k0": true]]`).
        // Same reasoning as the `Option[Map[…]]` arm directly above, one indirection
        // shorter: the element map's interior owns its key Strings, which breaks the
        // lenlist "one-level-exact" rule, so it takes its own class with the static
        // 2-level `$__drop_list_mb` (list -> the msb key sweep, which also rc_decs the
        // element block). Decided BEFORE the lenlist arm for the same reason the
        // Option sibling is — lenlist_elem_class returns None for a map element and it
        // would fall through to the nested-ownership wall.
        if matches!(elem_ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, b)
            if b.len() == 2 && matches!(b[0], Ty::String)
                && matches!(b[1], Ty::Bool | Ty::Int | Ty::Float))
        {
            return Some(ListElemDrop::MapSkv);
        }
        // An `Option[<record>]` element (`[some(Inner { n: 1 }), none]` — the codec
        // `lc: List[Inner?]` cell, #1134): each element is a 0-or-1 Option block whose
        // Some payload is the record — freed via the generated tag-aware
        // `$__drop_list_opt_<R>` (Some → the record's own free: one rc_dec for a flat
        // record, `$__drop_<R>` recursion for a rich one). Decided BEFORE the lenlist
        // arm (lenlist's flat_heap test would decline the record payload and fall to
        // the nested-ownership wall). Gated to a NON-generic Named record with a
        // resolvable layout; variants keep their own classes.
        if let almide_lang::types::Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, oa) = elem_ty
        {
            if let [Ty::Named(n, args)] = &oa[..] {
                if args.is_empty()
                    && self.custom_variant_type_name(&oa[0]).is_none()
                    && self.aggregate_field_tys(&oa[0]).is_some()
                {
                    return Some(ListElemDrop::OptRecord(n.as_str().to_string()));
                }
            }
        }
        if let Some(class) = crate::lower::lenlist_elem_class(elem_ty) {
            return Some(match class {
                crate::lower::CtorElemClass::Flat => ListElemDrop::CtorFlat,
                crate::lower::CtorElemClass::LenLoop => ListElemDrop::CtorLenLoop,
            });
        }
        if matches!(elem_ty, Ty::Fn { .. }) {
            // A `List[<Fn>]` LITERAL element (`[(x: Int) => x + 1, (x: Int) => x * 2]` —
            // #623's closure-parameter shape): each element is a fresh closure BLOCK (lifted
            // via `lift_lambda`, the SAME mechanism a call-argument lambda already uses),
            // freed per-element via the generated `$__drop_list_closure` (recurses into the
            // uniform `$__drop_closure` — required even for a non-capturing lambda, since the
            // LIST's TYPE alone (`List[(Int)->Int]`) does not preclude a capturing element).
            return Some(ListElemDrop::Closure);
        }
        // A SCALAR-ONLY aggregate element (a record/tuple whose every field is
        // inline scalar — snaidhm's `GlyphPoint`, ceangal's rect records). Its
        // block is FLAT: it owns no further heap, so the per-element `rc_dec` of
        // the Flat class IS its full free — exactly the physics `calls_p2`'s
        // `scalar_aggregate_elem` and C-183's nested sweep already rely on, and
        // the same reason a flat VARIANT element takes `CtorFlat` above.
        //
        // Classifying it here is what lets a `[xs[i]]` operand of a
        // `List[<flat record>]` concat go through the LITERAL builder (which has
        // the per-element arms) instead of the generic call-argument path, which
        // declined it and walled the enclosing accumulator loop (#888/#904).
        if is_heap_ty(elem_ty)
            && self
                .aggregate_field_tys(elem_ty)
                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                .is_some()
        {
            return Some(ListElemDrop::CtorFlat);
        }
        None
    }

    /// As [`Self::try_lower_record_list_literal`], but with an AUTHORITATIVE element type override.
    /// A `[{...}]` record LITERAL infers its element type STRUCTURALLY (`Ty::Record{fields}`) — never
    /// the NAMED record (the type checker leaves a record literal structural). So `record_drop_type_name`
    /// returns `None` and the literal declines. But the CONTEXT (a concat `acc + [{...}]` whose result is
    /// `List[Local]`) knows the element is the NAMED record. Threading that Named type makes BOTH the
    /// element MATERIALIZATION (by-name into the declared layout — `try_lower_record_construct` resolves
    /// `aggregate_field_tys(Named)` to the DECLARED field order) AND the list drop registration
    /// (`list_<Named>` → the generated `$__drop_list_<Named>`) use ONE consistent layout — no
    /// structural-vs-declared field-order mismatch (the soundness crux: a structural literal's field
    /// order need not equal the declared order, so freeing it via the declared `$__drop_<R>` would
    /// corrupt). `forced_elem = None` keeps the original structural-derived behavior.
    /// Register the freshly-built list's drop route for its element kind —
    /// the `variant_drop_handles` name (or set membership) `drop_op_for`
    /// dispatches on at scope end. Arms verbatim.
    fn register_list_drop_kind(&mut self, dst: ValueId, kind: ListElemDrop) {
        match kind {
            ListElemDrop::StrStr => {
                self.str_str_elem_lists.insert(dst);
            }
            ListElemDrop::ScalarAggregate | ListElemDrop::CtorFlat => {
                self.heap_elem_lists.insert(dst);
            }
            ListElemDrop::ListStr => {
                self.list_list_str_lists.insert(dst);
            }
            other => {
                let name = drop_route_name(other);
                self.variant_drop_handles.insert(dst, name);
            }
        }
    }

    pub(crate) fn try_lower_record_list_literal_as(
        &mut self,
        value: &IrExpr,
        forced_elem: Option<&Ty>,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else { return None };
        if elements.is_empty() {
            return None;
        }
        let elem_ty = match forced_elem {
            Some(t) => t.clone(),
            None => match &value.ty {
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
                _ => return None,
            },
        };
        // The element's drop kind (`classify_list_elem_drop`): a recursive-drop record
        // (`$__drop_list_<R>`), a `(String,String)` tuple (`Op::DropListStrStr` — the
        // map.entries / `[(k,v), …]` literal shape), OR an Option/Result CTOR element
        // (`[some(1), none]`, `[ok(1), err("x")]` — the collect-test shapes): a Flat class
        // (scalar payload — the per-element `rc_dec` of `DropListStr` is exact) or a LenLoop
        // class (owned handle slots — the generated `$__drop_list_lenlist`). Anything else →
        // `None` (the caller keeps the scalar / wall path).
        let Some(kind) = self.classify_list_elem_drop(&elem_ty) else {
            return None;
        };
        // Lower each element to an OWNED handle BEFORE the alloc (a field expr that allocates
        // must not interleave with the store sequence).
        let mut objs: Vec<ValueId> = Vec::with_capacity(elements.len());
        for e in elements {
            if let Some(obj) = self.lower_record_list_element(e, forced_elem, &elem_ty, kind.clone())? {
                objs.push(obj);
            }
        }
        let n = elements.len();
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
        for (i, obj) in objs.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(i) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let handle = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![obj] });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, handle] });
            self.ops.push(Op::Consume { v: obj });
            self.live_heap_handles.retain(|x| *x != obj);
        }
        self.register_list_drop_kind(dst, kind);
        // The literal is a REAL, POPULATED nested-ownership block (every element built
        // and moved in above) — admit the element-precise `xs[i]` borrow over the bound
        // var (`try_lower_heap_field_borrow`'s materialized_lists gate; the fan.settle
        // results literal's `rs[0] == ok(11)` eq reads exactly this).
        self.materialized_lists.insert(dst);
        self.live_heap_handles.push(dst);
        Some(dst)
    }

    /// A heap-element `List` LITERAL RETURNED in TAIL position (`fn aliases() ->
    /// List[(String, String)] = [("Ok", "ok"), …]`, `fn keyword_groups() ->
    /// List[KeywordGroup] = [KeywordGroup { … }, …]`) — build the SAME nested-ownership block
    /// as [`Self::try_lower_record_list_literal`] (each element moved in via
    /// `lower_owned_heap_field`, the recursive drop registered: `DropListStrStr` for a
    /// `(String, String)` list, `$__drop_list_<R>` via `variant_drop_handles="list_<R>"` for a
    /// `List[Record]`), then MOVE IT OUT as the return — i.e. REMOVE it from `live_heap_handles`
    /// so the function does NOT also emit a scope-end drop. The caller owns the returned list and
    /// frees it (its own recursive drop selected by `drop_op_for` from the SAME registered set).
    ///
    /// SOUNDNESS (no new op / no certificate change): identical to the tail Record / Tuple ctor
    /// move-out (`try_lower_record_construct` at the heap-tail head, `try_lower_tuple_construct`):
    /// the block is `i…m` — alloc (cert `i`), each element moved in (cert `m`), then the whole
    /// list moved out as the return (cert `m`). It is NEVER in `live_heap_handles`, so it is
    /// never among the scope-end `d`s — no double-free; and it is a REAL populated block (not a
    /// deferred `Opaque` EMPTY value), so no silent miscompile. The drop-set registration
    /// (`str_str_elem_lists` / `variant_drop_handles`) is keyed by the moved-out `ValueId` but is
    /// only ever consulted for a value that IS in `live_heap_handles` (scope-end) or is a
    /// subject/arm local — none apply to a moved-out tail result — so the stale entry is inert.
    pub(crate) fn try_lower_record_list_literal_tail(&mut self, value: &IrExpr) -> Option<ValueId> {
        let dst = self.try_lower_record_list_literal(value)?;
        // MOVE OUT: the caller owns + drops the returned list, so it must NOT also be released by
        // this function's scope-end drops (that would be a double-free). Exactly the `Var`/Tuple/
        // Record tail move-out — drop the tracking, keep the recursive-drop-set registration.
        self.live_heap_handles.retain(|h| *h != dst);
        Some(dst)
    }

    /// Construct a SPREAD record `R { ...base, f: override, … }`: a FRESH block of the
    /// SAME uniform-slot layout, where each declared field's slot is either the supplied
    /// OVERRIDE value or COPIED from `base`. The copy preserves value semantics — `base`
    /// is left fully intact (a scalar slot is a `Load` copy; a heap slot is a borrowed
    /// handle `Dup`'d so the new record owns a DISTINCT reference while `base` keeps its
    /// own). This is what makes `let b2 = Box { ...b, value: 8 }` print `b2.value=8
    /// b2.label=old` while `b.label` still reads `old` — both records own the same string
    /// content through independent reference counts.
    ///
    /// GATE: `base` must be a MATERIALIZED aggregate var (its slots are real — a deferred
    /// `Opaque` base would copy garbage), every declared field's CONCRETE type must be
    /// known (resolved from `base.ty`, which carries the instantiated generic args — the
    /// `Pair[Int,String]` concern), and every override value must lower to an owned-handle
    /// (heap) / scalar. Any miss → `None` (the binding falls back to the deferred Opaque,
    /// whose field reads then WALL — never wrong bytes).
    ///
    /// SOUNDNESS (no new op / no certificate change): identical to [`Self::try_lower_record_construct`]'s
    /// shape — the block is `i…d` (alloc then the masked `DropListStr`), each heap slot
    /// holds an OWNED handle that is `Consume`d (moved) into the slot (cert `m`). A copied
    /// heap field's owned handle comes from `Dup`-ing `base`'s borrowed slot handle (cert
    /// `a` then `m` = the balanced shape the checker already accepts for a List[String]
    /// element duplicated from another container). `base` is never consumed, so it remains
    /// the sole owner of its own slots (dropped once at its own scope end).
    /// Resolve the spread BASE to its block handle: a TRACKED, MATERIALIZED
    /// aggregate var (a deferred Opaque base would copy garbage), or a
    /// borrowed heap FIELD (`{ ...v._style, width: w }` — the container
    /// keeps ownership; the copy loop Dups each heap slot, so the borrowed
    /// base is read-only and stays valid through construction).
    fn spread_base_block(&mut self, base: &IrExpr) -> Option<ValueId> {
        let resolved = match &base.kind {
            IrExprKind::Var { id } if is_heap_ty(&base.ty) => {
                let src = self.value_or_global(*id).ok()?;
                if !self.materialized_aggregates.contains(&src) {
                    crate::trace::trace("ALMIDE_DBG_ELEM", || {
                        format!("[spread] base Var {id:?} not a materialized aggregate")
                    });
                    return None;
                }
                src
            }
            // A FIELD base (`{ ...v._style, width: w }` — the ceangal nested-style spread):
            // BORROW the inner block's handle from the materialized container's slot
            // (`try_lower_heap_field_borrow` gates on materialization at every level; the
            // container keeps ownership — the copy loop below Dups each heap slot, so the
            // borrowed base is read-only and stays valid through construction).
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if is_heap_ty(&base.ty) =>
            {
                self.try_lower_heap_field_borrow(base)?
            }
            _ => {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!(
                        "[spread] base kind {} (ty {:?}) outside the Var/Member subset",
                        crate::lower::kind_name(&base.kind),
                        base.ty
                    )
                });
                return None;
            }
                };
        Some(resolved)
    }

    pub(crate) fn try_lower_spread_record_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::SpreadRecord { base, fields } = &value.kind else {
            return None;
        };
        // The CANONICAL declaration-ordered (name, concrete-type) field list. The result's
        // type carries the instantiated generic args, so a `Pair[Int,String]` field `first: A`
        // resolves to `Int`. An unresolvable type ⇒ `None` ⇒ wall.
        let Some((names, tys)) = self.aggregate_field_tys(&value.ty) else {
            crate::trace::trace("ALMIDE_DBG_ELEM", || {
                format!("[spread] no aggregate layout for ty {:?}", value.ty)
            });
            return None;
        };
        let n = tys.len();
        if n == 0 || names.len() != n {
            return None;
        }
        let base_block = self.spread_base_block(base)?;
        // Per declared slot: the override expr (if the literal supplies it) or `None` (copy
        // from base). A field NOT in the declaration is a type error the checker rejects
        // upstream, so a supplied field always maps to a declared index.
        let mut overrides: Vec<Option<&IrExpr>> = vec![None; n];
        for (name, expr) in fields {
            let idx = names.iter().position(|nm| nm == name)?;
            overrides[idx] = Some(expr);
        }
        // The slot is heap iff the declared CONCRETE type is heap (the base's slot, and the
        // copy/override, follow that). A generic field already substituted to its concrete
        // type by `aggregate_field_tys`, so `is_heap_ty` is decisive here.
        let heap_slots: Vec<usize> = (0..n).filter(|&i| is_heap_ty(&tys[i])).collect();
        // Lower every OVERRIDE value FIRST (before the alloc) so an override expr that itself
        // allocates does not interleave with our store sequence. Copies read from `base` and
        // are emitted inline at store time (a pure Load / a Dup of a borrowed handle — neither
        // allocates a block that could interleave badly). Each entry: (slot-value, is-heap).
        // For a heap OVERRIDE the value is a fresh owned handle to Consume into the slot.
        let mut override_vals: Vec<Option<(ValueId, bool)>> = vec![None; n];
        for (i, ov) in overrides.iter().enumerate() {
            if let Some(expr) = ov {
                if is_heap_ty(&tys[i]) {
                    let Some(obj) = self.lower_owned_heap_field(expr) else {
                        crate::trace::trace("ALMIDE_DBG_ELEM", || {
                            format!(
                                "[spread] heap override {} ({}) declined",
                                names[i].as_str(),
                                crate::lower::kind_name(&expr.kind)
                            )
                        });
                        return None;
                    };
                    override_vals[i] = Some((obj, true));
                } else {
                    let Some(v) = self.lower_scalar_value(expr) else {
                        crate::trace::trace("ALMIDE_DBG_ELEM", || {
                            format!(
                                "[spread] scalar override {} ({}) declined",
                                names[i].as_str(),
                                crate::lower::kind_name(&expr.kind)
                            )
                        });
                        return None;
                    };
                    override_vals[i] = Some((v, false));
                }
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
        // The base's block handle, for the per-slot copy loads.
        let bh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(bh), args: vec![base_block] });
        for i in 0..n {
            let is_heap = is_heap_ty(&tys[i]);
            // The destination slot address.
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(i) as i64 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            // The value to store: an OVERRIDE's lowered value, or a COPY from base's slot.
            let (slot_val, consume_owned) = match override_vals[i].take() {
                Some((v, true)) => {
                    // A heap override: store its handle, then Consume the owned value (moved in).
                    let handle = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![v] });
                    (handle, Some(v))
                }
                Some((v, false)) => (v, None), // a scalar override: store directly.
                None => {
                    // Copy from base's slot at the same offset.
                    let baddr = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: baddr, op: IntOp::Add, a: bh, b: off });
                    if is_heap {
                        // BORROW base's slot handle, then Dup it: the new record owns a DISTINCT
                        // reference (cert `a`), so base's own slot stays valid and the new block's
                        // masked drop frees only its own reference (no double-free).
                        let borrowed = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(borrowed), args: vec![baddr] });
                        let owned = self.fresh_value();
                        self.ops.push(Op::Dup { dst: owned, src: borrowed });
                        self.live_heap_handles.push(owned);
                        let handle = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![owned] });
                        (handle, Some(owned))
                    } else {
                        // A scalar copy: a pure value Load (no ownership).
                        let v = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![baddr] });
                        (v, None)
                    }
                }
            };
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, slot_val],
            });
            if let Some(v) = consume_owned {
                self.ops.push(Op::Consume { v });
                self.live_heap_handles.retain(|x| *x != v);
            }
        }
        if !heap_slots.is_empty() {
            self.record_masks.insert(dst, heap_slots);
        }
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }
}

/// The `variant_drop_handles` route name for each name-routed element-drop
/// kind (the set-routed kinds — StrStr / ScalarAggregate / ListStr — are
/// registered directly in `register_list_drop_kind`). Names verbatim.
fn drop_route_name(kind: ListElemDrop) -> String {
    match kind {
        ListElemDrop::Record(rname) => format!("list_{rname}"),
        ListElemDrop::StrInt => "list_str_int".to_string(),
        ListElemDrop::IntStr => "list_int_str".to_string(),
        ListElemDrop::StrMapStr => "list_str_mss".to_string(),
        ListElemDrop::StrMapSkv => "list_str_msb".to_string(),
        ListElemDrop::OptMapSkv => "list_omb".to_string(),
        ListElemDrop::MapSkv => "list_mb".to_string(),
        ListElemDrop::StrListOpt => "list_str_mlo".to_string(),
        ListElemDrop::MapMlo => "list_map_mlo".to_string(),
        ListElemDrop::MapHval => "list_map_hval".to_string(),
        ListElemDrop::CtorLenLoop => "list_lenlist".to_string(),
        ListElemDrop::Closure => "list_closure".to_string(),
        ListElemDrop::StrClosure => "list_str_clo".to_string(),
        ListElemDrop::RecordInt(rname) => format!("list_{}_int", drop_fn_ident(&rname)),
        ListElemDrop::OptRecord(rname) => format!("list_opt_{}", drop_fn_ident(&rname)),
        ListElemDrop::StrVariant(vname) => format!("list_str_{}", drop_fn_ident(&vname)),
        ListElemDrop::StrStr
        | ListElemDrop::ScalarAggregate
        | ListElemDrop::CtorFlat
        | ListElemDrop::ListStr => {
            unreachable!("set-routed kinds are registered directly")
        }
    }
}

/// The `(String, V)` value shapes that follow the len@4-counted String-slot
/// discipline (see `is_map_msb_ty`): a scalar-valued string map, an
/// `Option[String]`, a String-erring `Result` over a flat Ok, or an all-String
/// tuple.
fn is_msb_pair_value(v: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(v, Ty::Applied(TypeConstructorId::Map, b)
        if b.len() == 2 && matches!(b[0], Ty::String)
            && matches!(b[1], Ty::Bool | Ty::Int | Ty::Float))
        || matches!(v, Ty::Applied(TypeConstructorId::Option, o)
            if o.len() == 1 && matches!(o[0], Ty::String))
        || matches!(v, Ty::Applied(TypeConstructorId::Result, e)
            if e.len() == 2 && matches!(e[1], Ty::String)
                && (!is_heap_ty(&e[0]) || matches!(e[0], Ty::String)))
        || matches!(v, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| matches!(t, Ty::String)))
}
