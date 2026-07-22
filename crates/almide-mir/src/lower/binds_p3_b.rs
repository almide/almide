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
        // A STRUCTURAL record element (`[{key: "x", val: "2"}]` in argument position —
        // the checker leaves the literal structural, so `record_drop_type_name` alone
        // declined it, calls_p2's List-arg wall): the synthesized anon-record drop
        // (`__drop_anonrec_<hash>`) covers it with the SAME field order the literal
        // materializes in — no declared-vs-structural order mismatch (the soundness
        // crux the named path guards).
        if let Some(rname) = self.record_or_anon_drop_type_name(elem_ty) {
            return Some(ListElemDrop::Record(rname));
        }
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
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
            && matches!(&tys[1], Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, b)
                if b.len() == 2 && matches!(b[0], Ty::String) && matches!(b[1], Ty::String)))
        {
            // A `(String, Map[String, String])` TUPLE element (the map_fold_heap_acc
            // nested-map literal's pairs list, `["k0": ["k0": "x"]]` desugared to
            // `map.from_list_msv([("k0", <inner map>)])`): slot1 is a MAP owning its own
            // String slots — the static `$__drop_list_str_mss` (map_msv.almd) frees
            // slot0 flat and sweeps the last-ref inner map (a flat rc_dec would leak
            // every inner key/value String). Checked BEFORE the generic StrVariant arm
            // (a Map is not a custom variant, so that arm's name lookup would decline).
            return Some(ListElemDrop::StrMapStr);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
            && matches!(&tys[1], Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                if b.len() == 1
                    && matches!(&b[0], Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, o)
                        if o.len() == 1 && matches!(o[0], Ty::Int))))
        {
            // A `(String, List[Option[Int]])` TUPLE element (compound_repr_interp's `deep`
            // pairs list, `["k": [some(1), none]]` desugared to `map.from_list_mlo([("k",
            // <lenlist>)])`): slot1 is a LIST owning its Option-block slots — the static
            // `$__drop_list_str_mlo` (map_mlo.almd) frees slot0 flat and sweeps the
            // last-ref inner list (a flat rc_dec would leak every Option block). Same
            // placement rationale as StrMapStr above.
            return Some(ListElemDrop::StrListOpt);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
            && matches!(tys[1], Ty::Fn { .. }))
        {
            // A `(String, <Fn>)` TUPLE element (`map.from_list([("a", () => …)])` — the
            // closure-valued map's pairs list): slot1 is a CLOSURE BLOCK whose captured
            // env a flat rc_dec would leak — the static `$__drop_list_str_clo` frees
            // slot0 flat (String rc_dec) and routes slot1 through `__drop_closure`.
            // Checked BEFORE the generic `(String, <non-flat heap>)` StrVariant arm
            // below, whose variant-name lookup would DECLINE a Fn slot (killing the
            // whole builder).
            return Some(ListElemDrop::StrClosure);
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
            && is_heap_ty(&tys[1]) && !self.is_flat_heap_tuple_slot(&tys[1]))
        {
            // A `(String, <RICH variant>)` TUPLE element (`[("x", ValInt(64)), ("y",
            // ValStr("s"))]` — generic_chain_unwrap_or's `List[(String, V)]` metadata
            // pairs, `type V = ValInt(Int) | ValStr(String)`): the MIRROR of `StrInt`,
            // but slot1 is NOT scalar — it is a variant needing its OWN recursive drop
            // (a `ValStr` payload owns a String). `DropListStrInt`'s render only ever
            // rc_decs slot0 and leaves slot1 UNTOUCHED (sound only when slot1 is truly
            // scalar) — reusing it here would silently LEAK every `ValStr` element's
            // String, so this is a genuinely new drop shape: a generated
            // `$__drop_list_str_<V>` (drop_sources.rs) frees slot0 (String, flat
            // rc_dec) AND recurses into slot1 via the variant's own already-generated
            // `$__drop_<V>` (V is a real, non-generic type — no shadow-type machinery
            // needed, unlike B117's generic-instantiation case).
            let Ty::Tuple(tys) = elem_ty else { unreachable!() };
            let Some(vname) = self.custom_variant_type_name(&tys[1]) else { return None };
            return Some(ListElemDrop::StrVariant(vname));
        }
        if matches!(elem_ty, Ty::Tuple(tys) if tys.len() == 2
            && !is_heap_ty(&tys[1])
            && self.record_or_anon_drop_type_name(&tys[0]).is_some())
        {
            // A `(<RECURSIVE-DROP record>, <scalar>)` TUPLE element (`[({name: "alice", age:
            // 30}, 1), …]` — compound_eq's `Map[P, Int]` from_list pairs): the RECORD mirror
            // of `StrVariant`. `DropListStrInt` only rc_decs slot0 one level — P owns a String
            // field, so a flat rc_dec LEAKS it; slot0 must recurse via `$__drop_<R>`. The
            // element's record slot is FORCED to this declared/classified type at construction
            // (below), so classification name, construction layout, and the generated
            // `$__drop_list_<R>_int` teardown all key on ONE name — the mismatch that produced
            // the earlier attempt's dangling `$__drop_list_anonrec_<hash>_int`.
            let Ty::Tuple(tys) = elem_ty else { unreachable!() };
            let Some(rname) = self.record_or_anon_drop_type_name(&tys[0]) else { return None };
            return Some(ListElemDrop::RecordInt(rname));
        }
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
            // When the element type is forced (a structural record LITERAL in a `List[Named]` context),
            // materialize the element AS the Named type so `try_lower_record_construct` lays it out by
            // the DECLARED field order (matching the `$__drop_list_<Named>` teardown). Field-by-name
            // assignment makes this order-correct regardless of the literal's source field order.
            let forced_e;
            let e_ref = match forced_elem {
                Some(ft) if matches!(e.kind, IrExprKind::Record { .. }) => {
                    forced_e = IrExpr { ty: ft.clone(), ..e.clone() };
                    &forced_e
                }
                _ => e,
            };
            // A CTOR-class element (`some(1)`, `err("x")`) materializes through the Option/Result
            // ctor builder (a fresh OWNED wrapper block; the ctor arms leave tracking to callers,
            // so push it for the uniform Consume below). A Var/call element of the SAME type takes
            // `lower_owned_heap_field` (Dup / fresh CallFn result) — the drop class is TYPE-driven,
            // so both produce blocks the registered list drop frees exactly.
            if matches!(kind, ListElemDrop::ListStr) {
                // An inner `List[String]` LITERAL element builds through the str-list
                // builder (fresh owned, tracked by it); a Var/call element of the exact
                // element type takes the generic owned-field path below. A type-rewritten
                // (never-err-lifted) element declines — the same guard as the ctor class.
                if matches!(e_ref.kind, IrExprKind::List { .. }) {
                    if let Some(obj) = self.try_lower_str_list_literal(e_ref) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                    return None;
                }
                if e_ref.ty != elem_ty {
                    return None;
                }
            }
            if matches!(kind, ListElemDrop::ScalarAggregate) {
                // An inner `List[<scalar>]` LITERAL element builds through the flat
                // slots builder (fresh owned; the uniform Consume below moves it in).
                if let IrExprKind::List { elements: iels } = &e_ref.kind {
                    let iels = iels.clone();
                    if let Some(obj) = self.try_lower_scalar_list_slots(&iels) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                    return None;
                }
                if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
                    let tels = tels.clone();
                    if let Some(obj) = self.try_lower_scalar_tuple_construct(&tels) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                    return None;
                }
            }
            if matches!(kind, ListElemDrop::RecordInt(_)) {
                // The tuple's record slot is a STRUCTURAL literal (`({name: …, age: …}, 1)`) —
                // FORCE it to the classified type (the forced_elem precedent, extended into the
                // tuple slot) so `lower_owned_heap_field`'s recursive-record arm constructs the
                // SAME layout the registered `$__drop_list_<R>_int` tears down. A non-literal
                // slot must already carry the exact classified type; anything else declines.
                if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
                    let Ty::Tuple(tys) = &elem_ty else { return None };
                    let mut tels = tels.clone();
                    if matches!(tels[0].kind, IrExprKind::Record { .. }) {
                        tels[0].ty = tys[0].clone();
                    } else if tels[0].ty != tys[0] {
                        return None;
                    }
                    if let Some(obj) = self.try_lower_tuple_construct(&tels) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                }
                return None;
            }
            if matches!(kind, ListElemDrop::StrInt | ListElemDrop::IntStr | ListElemDrop::StrVariant(_) | ListElemDrop::StrMapStr | ListElemDrop::StrListOpt) {
                // A `(String, Int)` / `(Int, String)` / `(String, <rich variant>)` TUPLE LITERAL
                // element builds through the general masked-tuple builder (String slot fresh
                // OWNED + moved in, the other slot a scalar store OR — for `StrVariant` — a
                // fresh OWNED variant ctor block via `lower_owned_heap_field`'s existing
                // ctor-call dispatch; `try_lower_tuple_construct` already handles arbitrary
                // heap/scalar slot mixes for other callers, so no new construction path is
                // needed here). The list's OWN drop (registered below via
                // `variant_drop_handles`) frees each tuple's slots recursively, so the tuple's
                // own `record_masks` entry never scope-end-fires — mirrored from the
                // `(Int, String)` precedent in calls_p2.rs/binds.rs.
                if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
                    let tels = tels.clone();
                    if let Some(obj) = self.try_lower_tuple_construct(&tels) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                    return None;
                }
            }
            if matches!(kind, ListElemDrop::Closure) {
                // A LAMBDA literal element: lift it to a fresh `__lambda_*` fn + closure block
                // via the SAME proven mechanism a call-argument lambda already uses (calls_p2.rs).
                if let IrExprKind::Lambda { params, body, .. } = &e_ref.kind {
                    if let Some(obj) = self.lift_lambda(params, body) {
                        if !self.live_heap_handles.contains(&obj) {
                            self.live_heap_handles.push(obj);
                        }
                        objs.push(obj);
                        continue;
                    }
                    return None;
                }
                // A non-lambda element (a Var holding a closure / a call returning one) must
                // carry the list's element type; anything else declines rather than storing a
                // mismatched value into a closure-drop-typed slot.
                if e_ref.ty != elem_ty {
                    return None;
                }
            }
            if matches!(kind, ListElemDrop::CtorFlat | ListElemDrop::CtorLenLoop) {
                if let Some(obj) = self.try_lower_option_ctor(e_ref, &elem_ty) {
                    if !self.live_heap_handles.contains(&obj) {
                        self.live_heap_handles.push(obj);
                    }
                    objs.push(obj);
                    continue;
                }
                // A non-ctor element (a Var / call) must CARRY the list's element type — a
                // never-err LIFTED effect call (`[step(), step()]`, autotry_construction) has
                // its call type rewritten to the RAW payload (Int), so lowering it here would
                // store a SCALAR where the registered drop expects an owned handle (invalid
                // wasm + an unacquired `m` witness — the PCC gate caught exactly this).
                // Decline → the caller walls, never a wrong byte.
                if e_ref.ty != elem_ty {
                    return None;
                }
            }
            objs.push(self.lower_owned_heap_field(e_ref)?);
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
        match kind {
            ListElemDrop::Record(rname) => {
                self.variant_drop_handles.insert(dst, format!("list_{rname}"));
            }
            ListElemDrop::StrStr => {
                self.str_str_elem_lists.insert(dst);
            }
            ListElemDrop::StrInt => {
                self.variant_drop_handles.insert(dst, "list_str_int".to_string());
            }
            ListElemDrop::IntStr => {
                self.variant_drop_handles.insert(dst, "list_int_str".to_string());
            }
            ListElemDrop::RecordInt(rname) => {
                // → the GENERATED `$__drop_list_<R>_int` (drop_sources.rs — the same
                // unconditional per-recursive-record / per-anon-record loops that already emit
                // `$__drop_list_<R>`): per element, recurse into slot0 via `$__drop_<R>`, then
                // free the tuple block; slot1 is scalar (nothing to free).
                let rname_fn = drop_fn_ident(&rname);
                self.variant_drop_handles.insert(dst, format!("list_{rname_fn}_int"));
            }
            ListElemDrop::StrVariant(vname) => {
                // Routes through `Op::DropVariant`'s generic `variant_drop_handles` fallback
                // (drop_op_for, mod_p3.rs) to `$__drop_<ty>` — `ty` = `list_str_<vname>` names
                // the GENERATED `$__drop_list_str_<vname>` (drop_sources.rs, mirroring the
                // `$__drop_list_<V>`/`$__drop_res_<V>` generation this session's B117 extended).
                let vname_fn = drop_fn_ident(&vname);
                self.variant_drop_handles.insert(dst, format!("list_str_{vname_fn}"));
            }
            ListElemDrop::StrMapStr => {
                self.variant_drop_handles.insert(dst, "list_str_mss".to_string());
            }
            ListElemDrop::StrListOpt => {
                self.variant_drop_handles.insert(dst, "list_str_mlo".to_string());
            }
            ListElemDrop::MapMlo => {
                self.variant_drop_handles.insert(dst, "list_map_mlo".to_string());
            }
            ListElemDrop::MapHval => {
                self.variant_drop_handles.insert(dst, "list_map_hval".to_string());
            }
            ListElemDrop::ScalarAggregate => {
                self.heap_elem_lists.insert(dst);
            }
            ListElemDrop::ListStr => {
                self.list_list_str_lists.insert(dst);
            }
            // Flat ctor elements (Option[scalar]) free exactly under the per-element `rc_dec`
            // of the masked `DropListStr`; LenLoop elements route to the generated
            // `$__drop_list_lenlist` (injected iff the pre-scan saw this literal — the shared
            // `lenlist_elem_class` keeps the two decisions identical by construction).
            ListElemDrop::CtorFlat => {
                self.heap_elem_lists.insert(dst);
            }
            ListElemDrop::CtorLenLoop => {
                self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
            }
            ListElemDrop::Closure => {
                self.variant_drop_handles.insert(dst, "list_closure".to_string());
            }
            ListElemDrop::StrClosure => {
                self.variant_drop_handles.insert(dst, "list_str_clo".to_string());
            }
        }
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
    pub(crate) fn try_lower_spread_record_construct(&mut self, value: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::SpreadRecord { base, fields } = &value.kind else {
            return None;
        };
        // The CANONICAL declaration-ordered (name, concrete-type) field list. The result's
        // type carries the instantiated generic args, so a `Pair[Int,String]` field `first: A`
        // resolves to `Int`. An unresolvable type ⇒ `None` ⇒ wall.
        let (names, tys) = self.aggregate_field_tys(&value.ty)?;
        let n = tys.len();
        if n == 0 || names.len() != n {
            return None;
        }
        // The base must be a TRACKED, MATERIALIZED aggregate var — its slots are real, so a
        // copy reads the right value (a deferred Opaque base would copy garbage). Resolve its
        // block handle.
        let base_block = match &base.kind {
            IrExprKind::Var { id } if is_heap_ty(&base.ty) => {
                let src = self.value_or_global(*id).ok()?;
                if !self.materialized_aggregates.contains(&src) {
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
            _ => return None,
        };
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
                    let obj = self.lower_owned_heap_field(expr)?;
                    override_vals[i] = Some((obj, true));
                } else {
                    let v = self.lower_scalar_value(expr)?;
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
