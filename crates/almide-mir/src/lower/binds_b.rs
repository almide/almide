impl LowerCtx {

    /// The element-shape → drop-route classification for
    /// [`Self::try_lower_str_list_literal`]'s freshly-allocated `list`. Verbatim extraction
    /// (guard-clause flattening) of the former inline else-if chain, no behavior change — see
    /// docs/roadmap/active/code-health-codopsy.md.
    #[allow(clippy::too_many_arguments)]
    fn classify_str_list_literal_drop(
        &mut self,
        list: ValueId,
        elem_value: bool,
        elem_str_value: bool,
        elem_int_str: bool,
        elem_str_int: bool,
        elem_list_str: bool,
        elem_list_flat: bool,
        elem_rich_variant: &Option<String>,
        elem_recdrop: &Option<String>,
    ) {
        if elem_value {
            self.value_elem_lists.insert(list);
            return;
        }
        if elem_str_value {
            self.str_value_elem_lists.insert(list);
            return;
        }
        if elem_int_str {
            self.variant_drop_handles.insert(list, "list_int_str".to_string());
            return;
        }
        if elem_str_int {
            self.variant_drop_handles.insert(list, "list_str_int".to_string());
            return;
        }
        if elem_list_str || elem_list_flat {
            // elem_list_flat: each element is a matrix-shaped two-level block — the SAME
            // DropListListStr sweep (rc_dec each element's flat sub-blocks + the element,
            // then the list) is its exact recursive free.
            self.list_list_str_lists.insert(list);
            return;
        }
        if let Some(vname) = elem_rich_variant {
            // RECURSIVE per-element drop via the generated `$__drop_list_<V>`.
            self.variant_drop_handles.insert(list, format!("list_{vname}"));
            return;
        }
        if let Some(rname) = elem_recdrop {
            self.variant_drop_handles.insert(list, format!("list_{rname}"));
            return;
        }
        self.heap_elem_lists.insert(list);
    }

    /// Lower a `List[String]` LITERAL to an alloc_list_str + per-element move-in. Each element is
    /// stored into a nested-ownership `DynListStr` (freed recursively via `DropListStr` at scope end,
    /// cert `i`+`d`). Element ownership by kind:
    /// - a LitStr / ConcatStr is a FRESH owned String (cert `i`), MOVED in (store handle + `Consume`);
    /// - a `Var` binds a STILL-LIVE owned String the list must not steal — it gets its OWN reference
    ///   via `Dup` (cert `a`/+1), then that fresh handle is `Consume`d into the list. The original var
    ///   keeps its reference (its scope-end drop stays balanced), and the list owns a distinct one
    ///   (DropListStr releases it) — no double-free, no leak. (`[e0, e1]` of `string.repeat` results,
    ///   `[a, a]` of a computed `a` — the same var may appear twice, each occurrence its own `Dup`.)
    /// GATED to those element kinds; any other (a heap-returning call as a bare element, a member
    /// access) defers. Gate-first so no partial emission.
    pub(crate) fn try_lower_str_list_literal(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &value.kind else {
            return None;
        };
        // A `List[String]` OR a `List[ScalarAggregate]` whose every element is a SCALAR-only
        // record OR tuple literal — all are NESTED-OWNERSHIP lists (i64 slots holding owned heap
        // handles, recursively freed via `DropListStr`'s per-slot `rc_dec`). A scalar record/tuple
        // is a FLAT block (no inner heap slots), so `rc_dec` frees it correctly (no String-specific
        // recursion). This is what makes `[Point{..}, Point{..}]` (then `list.map(…, p => p.x)`)
        // AND `[(1, 100), (127, 300)]` (then a `for (x, y) in …`) materialize as a REAL list of the
        // right length. A `List` of any OTHER heap element (a heap-field record/tuple, a List, a
        // call result) defers — a heap-field aggregate needs the masked recursive drop this builder
        // (a flat per-slot `rc_dec`) does not emit, so its inner heap would leak; gated out by
        // `scalar_slots` (which is `None` for any non-scalar field).
        let elem_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String));
        let elem_scalar_aggregate = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.aggregate_field_tys(&a[0])
                    .and_then(|(_, tys)| layout::scalar_slots(&tys)).is_some());
        // A `List[Value]` (`[value.int(1), value.str("a")]`) — its slots hold OWNED dynamic Values,
        // each freed RECURSIVELY at scope end via `Op::DropListValue` (`$__drop_value` per element),
        // so a Str/Array element's nested payload is reclaimed. Elements are fresh-owned ctor CALLS
        // (Module `value.*` / a Named `Value`-returning fn) or a tracked Value Var (Dup'd).
        let elem_value = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_value_ty(&a[0]));
        // A `List[(String, Value)]` (`[(key, val)]` — the yaml `pairs + [(k,v)]` append) — each element is a
        // HEAP-FIELD (String, Value) tuple, materialized via `try_lower_tuple_construct` (rc-owning both
        // fields) and reclaimed RECURSIVELY at scope end via `Op::DropListStrValue` (per tuple: rc_dec the
        // String + `$__drop_value` the Value). A flat `DropListStr` would leak each tuple's payloads.
        let elem_str_value = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && is_value_ty(&tys[1])));
        // A `List[List[String]]` (`[cur]` — the csv `rows + [cur]` singleton) — each element is an
        // owned `List[String]` block (Dup'd in, the new list co-owns each row), reclaimed RECURSIVELY
        // at scope end via `Op::DropListListStr` (the nested cell + row free). A flat `DropListStr`
        // would only rc_dec each row HANDLE, leaking the cell Strings.
        let elem_list_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)));
        // A `List[List[scalar]]` literal (`[[1.0, 2.0], [3.0]]` — the nn Matrix seed):
        // each inner list is a FLAT scalar block whose rc_dec IS its full free, so the
        // outer list's per-slot DropListStr reclaims everything — same ownership shape
        // as List[String].
        let elem_list_scalar = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && !is_heap_ty(&b[0])));
        // A `List[List[List[scalar]]]` / `List[Matrix]` literal (`[matrix.to_lists(a),
        // matrix.to_lists(b)]` — the nn concat_rows flatten argument): each element is a
        // TWO-LEVEL block (a matrix: its slots hold owned flat row blocks), so the list's
        // scope-end drop must be the nested `DropListListStr` (rc_dec each element's row
        // slots + the element block, then the list — `list_list_str_lists`). Elements are
        // fresh-owned matrix.* / to_lists CALLS moved in, or tracked Vars Dup'd in.
        let elem_list_flat = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Matrix
                | Ty::Applied(TypeConstructorId::Matrix, _))
        ) || matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(&b[0],
                    Ty::Applied(TypeConstructorId::List, c) if c.len() == 1 && !is_heap_ty(&c[0]))));
        // A `List[(<scalar>, String)]` (`[(i, line)]` — list.enumerate; `[(true, "yes")]` —
        // the bool-key map literal): each element is a (scalar @12, String @20) tuple,
        // materialized via `try_lower_tuple_construct` and reclaimed RECURSIVELY at scope end
        // via `$__drop_list_int_str` (per tuple: rc_dec the String only — correct for ANY
        // scalar key, the slot layout is identical). A flat `DropListStr` would leak each
        // tuple's String.
        let elem_int_str = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && matches!(tys[1], Ty::String)));
        // A `(String, Int)` TUPLE element (`[("alpha", 1), …]` — the tokenizer vocab
        // pairs) — `DropListStrInt` rc_decs each tuple's String slot0 only.
        let elem_str_int = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
                Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Int)));
        // A HEAP-FIELD record element with a generated recursive drop (`[{key: p, val: "1"}]`
        // — porta's List[EnvVar] literal): each element materializes via the full record
        // builder (rc-owning its heap fields) and the list drops via `$__drop_list_<R>`
        // (each element → `$__drop_<R>`), exactly the concat-side `record_elem` route.
        let elem_recdrop = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                self.record_drop_type_name(&a[0])
            }
            _ => None,
        };
        // A `List[V]` whose element `V` is a FLAT custom variant (every ctor scalar-only — a nullary
        // enum like `Capability = | CapIO | CapProcess`, or a scalar-payload variant): each element is
        // a fresh OWNED tag-block (`try_lower_variant_ctor`, cert `i`) moved into the slot; the list's
        // scope-end `DropListStr` `rc_dec`s each element + the block (a flat variant block owns no inner
        // handle, so `rc_dec` is its full free — the SAME proven `List[String]` cert). A variant with a
        // `String`/nested/`List` field is NOT flat (`is_flat_variant_ty` = false) and stays walled.
        let elem_flat_variant = matches!(&value.ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && self.variant_layouts.is_flat_variant_ty(&a[0]));
        // A RICH (recursive-drop) variant element (`[instr_r.val]` — the `acc + [instr_r.val]` singleton
        // operand, `instr_r.val: Instr`). Each element is a fresh OWNED ref (Dup of the borrowed field /
        // Var); the list's `$__drop_list_<V>` frees each RECURSIVELY via `$__drop_<V>` — a flat per-slot
        // `rc_dec` (`DropListStr`) would leak each element's nested `List[Instr]`.
        let elem_rich_variant: Option<String> = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => {
                self.variant_layouts.is_rich_variant_ty(&a[0], &|rn| {
                    crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                })
            }
            _ => None,
        };
        // An EMPTY literal of an admitted class builds the same zero-length block
        // (`ok([])` — the tail-duplicated else-arm's Result payload, porta
        // resolve_env). The element loop below is vacuous; DropListStr over a
        // len-0 block frees just the block.
        if !elem_str && !elem_scalar_aggregate && !elem_value && !elem_str_value && !elem_list_str
            && !elem_int_str && !elem_str_int && !elem_flat_variant && elem_rich_variant.is_none()
            && elem_recdrop.is_none() && !elem_list_scalar && !elem_list_flat
        {
            return None;
        }
        // Gate: every element must be a fresh-owned String (LitStr/ConcatStr) OR a tracked
        // heap Var (so we can Dup it) OR — for an aggregate list — a scalar-only record/tuple
        // LITERAL we can materialize OR — for a value list — a fresh-owned Value CALL. A Var whose
        // value isn't tracked here cannot be Dup'd → defer.
        let all_lowerable = elements.iter().all(|e| match &e.kind {
            IrExprKind::LitStr { .. } | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
            // A `${...}` interpolation element (`["", "[[${emit_path(...)}]]"]` — the toml
            // emit_sections shape): a fresh owned String, moved into the slot exactly like a concat.
            IrExprKind::StringInterp { .. } => elem_str,
            IrExprKind::Var { id } => self.value_of.contains_key(id),
            // An inner scalar-list LITERAL (`[1.0, 2.0]` in a List[List[Float]]) —
            // buildable iff every inner element is a lowerable scalar (the flat
            // builder itself re-checks; a non-scalar inner defers there).
            IrExprKind::List { .. } => elem_list_scalar,
            // A RECORD-CTOR VARIANT element (`[Click { x, y }, KeyPress { key }, …]` — the
            // event-list shape): a `Record` literal whose NAME is a registered constructor is
            // a TAGGED variant value, materialized via `try_lower_variant_ctor` below (NOT the
            // plain-record path). Gated on the list being a flat/rich variant list.
            IrExprKind::Record { name: Some(n), .. }
                if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(n.as_str()) =>
            {
                true
            }
            IrExprKind::Record { .. } => elem_scalar_aggregate || elem_recdrop.is_some(),
            IrExprKind::Tuple { .. } => elem_scalar_aggregate || elem_str_value || elem_int_str || elem_str_int,
            // A FLAT-variant CONSTRUCTOR element (`[CapIO, CapProcess]`) — a Named call whose name is a
            // registered constructor, materialized via `try_lower_variant_ctor` below.
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                true
            }
            // A flat-variant FIELD EXTRACTION element (`acc + [r.val]`) — admitted ONLY when the field
            // BORROW resolves (a tracked/param heap-aggregate container), so the build loop's `?` never
            // fails mid-build (which would leak partial ops). Mirrors `try_lower_heap_field_borrow`'s
            // container gate so the two never disagree.
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
                if (elem_flat_variant || elem_rich_variant.is_some() || elem_str || elem_value)
                    && is_heap_ty(&e.ty) =>
            {
                self.heap_field_container_tracked(object)
            }
            IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, .. } => {
                // A Value-returning ctor call (elem_value), OR — for a List[String] — a String-returning
                // call element (`[string.slice(s,a,b)]` in `acc + [string.slice(…)]`, the dominant yaml
                // append shape): a fresh owned String, moved into the slot. `e.ty` is String here.
                // A matrix-shaped call element (`[matrix.to_lists(a), …]` — elem_list_flat) is the
                // same fresh-owned move-in; the list's DropListListStr reclaims it two levels deep.
                (elem_value && is_value_ty(&e.ty)) || elem_str || elem_list_flat
            }
            _ => false,
        });
        if !all_lowerable {
            return None;
        }
        let ptr = crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT };
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc { dst: list, repr: ptr, init: Init::DynListStr { len: n } });
        // A `List[Value]` drops via `Op::DropListValue` (recursive `$__drop_value` per element); a
        // String/aggregate list via the flat-element `Op::DropListStr`. Marking the right set is what
        // makes the scope-end drop reclaim each element's nested payload (no leak).
        self.classify_str_list_literal_drop(
            list,
            elem_value,
            elem_str_value,
            elem_int_str,
            elem_str_int,
            elem_list_str,
            elem_list_flat,
            &elem_rich_variant,
            &elem_recdrop,
        );
        self.materialized_lists.insert(list);
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, elem) in elements.iter().enumerate() {
            let ev = match &elem.kind {
                IrExprKind::LitStr { value: s } => {
                    let obj = self.fresh_value();
                    self.ops.push(Op::Alloc { dst: obj, repr: ptr, init: Init::Str(s.clone()) });
                    obj
                }
                // A Var element: acquire a fresh owned reference (Dup) the list will own; the original
                // binding keeps its own reference. The dup is then Consume'd (moved) into the slot.
                IrExprKind::Var { id } => {
                    let src = *self.value_of.get(id)?;
                    let dup = self.fresh_value();
                    self.ops.push(Op::Dup { dst: dup, src });
                    dup
                }
                // A RECORD-CTOR VARIANT element (`Click { x, y }` where `Click` is a registered
                // ctor) — materialize the tagged variant block via `try_lower_variant_ctor` (NOT
                // the plain-record path), moved into the slot; the list's `$__drop_list_<V>` frees
                // each element recursively. Checked BEFORE the plain-record arms.
                IrExprKind::Record { name: Some(n), .. }
                    if (elem_flat_variant || elem_rich_variant.is_some())
                        && self.variant_layouts.ctor_to_type.contains_key(n.as_str()) =>
                {
                    self.try_lower_variant_ctor(elem)?
                }
                // A scalar-only record literal element — materialize a fresh OWNED record
                // block (`try_lower_scalar_record_construct`, cert `i`), moved into the slot.
                IrExprKind::Record { .. } if elem_recdrop.is_some() => {
                    self.try_lower_record_construct(elem)?
                }
                // An inner scalar-list LITERAL element (`[1.0, 2.0]` inside a
                // List[List[Float]] literal) — the flat scalar-slot builder yields a
                // fresh owned block moved into the outer slot.
                IrExprKind::List { elements: inner } if elem_list_scalar => {
                    self.try_lower_scalar_list_slots(inner)?
                }
                IrExprKind::Record { .. } => self.try_lower_scalar_record_construct(elem)?,
                // A scalar-only tuple literal element (`(1, 100)`) — materialize a fresh OWNED
                // flat 2-slot block (`try_lower_scalar_tuple_construct`, cert `i`), moved into the
                // slot. The SAME flat shape as a scalar record, so the list's per-slot `rc_dec`
                // frees it correctly.
                // A HEAP-FIELD `(String, Value)` tuple element (`(key, val)`) — materialize a fresh OWNED
                // mixed 2-slot block (`try_lower_tuple_construct`, rc-owning both fields), moved into the
                // slot; the list's `DropListStrValue` reclaims each tuple recursively.
                IrExprKind::Tuple { elements: tup_elems } if elem_str_value || elem_int_str || elem_str_int => {
                    self.try_lower_tuple_construct(tup_elems)?
                }
                IrExprKind::Tuple { .. } => self.try_lower_scalar_tuple_construct_for_elem(elem)?,
                // A heap-returning CALL element — a fresh OWNED value MOVED into the slot. A `Value`
                // ctor (`value.int(1)`) for a List[Value]; a String-returning call (`string.slice(s,a,b)`
                // — the dominant yaml `acc + [string.slice(…)]` append) for a List[String]. Module via
                // the pure-call path (→ a registered CallFn like `string.slice`), Named via CallFn. The
                // list's recursive drop (DropListValue / DropListStr) frees each at scope end.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if elem_value || elem_str || elem_list_flat =>
                {
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &elem.ty).ok()?
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if elem_value || elem_str || elem_list_flat =>
                {
                    let lowered = self.lower_call_args(args).ok()?;
                    let obj = self.fresh_value();
                    let repr = repr_of(&elem.ty).ok()?;
                    self.ops.push(Op::CallFn {
                        dst: Some(obj),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(repr),
                    });
                    obj
                }
                // A FLAT-variant CONSTRUCTOR element (`CapIO`) — materialize the fresh OWNED tag-block
                // (`try_lower_variant_ctor`, cert `i`) and move it into the slot. The block owns no
                // inner handle (flat), so the list's `DropListStr` `rc_dec` is its full free.
                IrExprKind::Call { target: CallTarget::Named { name }, .. }
                    if (elem_flat_variant || elem_rich_variant.is_some())
                    && self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
                {
                    self.try_lower_variant_ctor(elem)?
                }
                // A flat-variant FIELD EXTRACTION element (`acc + [r.val]`, `r.val: ValType` — the
                // wasm-binary `read_vec_valtype_acc` recursive accumulator, where the unwrapped result
                // record `r` carries >1 heap field so `r.val` stays a `Member` rather than folding to a
                // `Var`). BORROW the field handle (the container keeps owning it, freed by its own
                // recursive drop) and `Dup` a fresh OWNED reference to MOVE into the slot. The list
                // co-owns the rc-inc'd block; its `DropListStr` `rc_dec` balances the `Dup`, and a flat
                // variant block owns no inner handle so `rc_dec` is its full free — no double-free
                // (rc-aware). Cert-identical to the `Var` element case (the extra `LoadHandle` is a
                // cert-neutral prim load): `Dup` = `a`, `Consume` = the move into the list `i…m`.
                // A heap-FIELD String/Value element (`opts.profile` in `args + ["--profile",
                // opts.profile]`, the porta serialize_opts shape) — BORROW the field handle (the
                // container keeps owning it, freed by its own drop) and `Dup` a fresh OWNED reference
                // to MOVE into the slot. The list co-owns the rc-inc'd String/Value; its DropListStr /
                // DropListValue `rc_dec`s it, balancing the `Dup` — no double-free (rc-aware), no leak.
                // Cert-identical to the `Var` element case (the `Dup` = `a`, the move into the list =
                // `m`); the extra `LoadHandle` is a cert-neutral prim load.
                IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                    if elem_flat_variant || elem_rich_variant.is_some() || elem_str || elem_value =>
                {
                    let borrowed = self.try_lower_heap_field_borrow(elem)?;
                    let dup = self.fresh_value();
                    self.ops.push(Op::Dup { dst: dup, src: borrowed });
                    dup
                }
                // A `${...}` interpolation element → a fresh owned String via the interp concat chain.
                IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts)?,
                _ => self.try_lower_concat_str(elem)?,
            };
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let slot = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: slot, op: crate::IntOp::Add, a: h, b: off });
            let eh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(eh), args: vec![ev] });
            self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![slot, eh] });
            self.ops.push(Op::Consume { v: ev });
        }
        Some(list)
    }
}

include!("binds_p2.rs");
include!("binds_p2_b.rs");
include!("binds_p2_c.rs");
include!("binds_p3.rs");
include!("binds_p3_b.rs");
include!("binds_p4.rs");
include!("binds_p4_b.rs");
