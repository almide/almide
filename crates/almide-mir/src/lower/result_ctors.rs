impl LowerCtx {
    /// Construct a `Result[(R, Int), String]` `ok((R {{…}}, n))` / `err(<String>)` — the
    /// gguf parse_header shape. Ok materializes the owned record then a 2-slot tuple
    /// block owning it (record handle @12, Int @20) and wraps it; the wrapper's drop
    /// recurses via the generated `$__drop_tup_int_<R>` (`resrec:tup_int_<R>`).
    /// Is `ty` `Result[(R, Int), String]` for a RECORD R (recursive-drop or flat)?
    pub(crate) fn is_rec_int_result_ty(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2
                && matches!(a[1], Ty::String)
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[1], Ty::Int)
                    && self.aggregate_field_tys(&ts[0]).is_some()
                    && !matches!(ts[0], Ty::String)))
    }

    /// The `Option[<leaf>]` Ok type of a `Result[Option[L], String]` plus whether
    /// that leaf is a String (the alternative being a plain scalar). `None` for
    /// any other shape — a heap leaf has no flat block form here.
    fn option_leaf_str_or_scalar<'t>(result_ty: &'t Ty) -> Option<(&'t Ty, bool)> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = result_ty else { return None };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return None;
        }
        let ok_ty = &a[0];
        let Ty::Applied(TypeConstructorId::Option, oa) = ok_ty else { return None };
        if oa.len() != 1 {
            return None;
        }
        let is_str = matches!(oa[0], Ty::String);
        let is_scalar = matches!(oa[0], Ty::Int | Ty::Float | Ty::Bool);
        (is_str || is_scalar).then_some((ok_ty, is_str))
    }

    /// The `(record, Int)` Ok payload of a `Result[(R, Int), String]`, when the
    /// record half is a real aggregate (not a String). `None` for any other
    /// Result shape.
    fn rec_int_ok_payload(&self, result_ty: &Ty) -> Option<Ty> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = result_ty else { return None };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return None;
        }
        let Ty::Tuple(ts) = &a[0] else { return None };
        let ok = ts.len() == 2
            && matches!(ts[1], Ty::Int)
            && self.aggregate_field_tys(&ts[0]).is_some()
            && !matches!(ts[0], Ty::String);
        ok.then(|| ts[0].clone())
    }

    /// Wrap a built payload as the Result block: a RECURSIVE-drop record routes
    /// through the generated `$__drop_tup_int_<R>` wrapper, a FLAT one REUSES
    /// `DropResultStrInt` (its tuple frees exactly like a `(String, Int)`).
    fn wrap_rec_int_result(
        &mut self,
        payload: ValueId,
        repr: crate::Repr,
        is_err: bool,
        drop_fn: Option<&str>,
    ) -> ValueId {
        if let Some(df) = drop_fn {
            return self.materialize_result_aggregate(payload, repr, is_err, df.to_string());
        }
        let obj = self.materialize_result_str(payload, repr, is_err, false);
        self.value_drops.get_mut(&obj).map(|d| d.flat_elems = false);
        self.value_drops.entry(obj).or_default().str_int_result = true;
        obj
    }

    /// Roll the ops and live handles this attempt pushed back to their marks and
    /// decline, so the caller sees the lowerer exactly as it was.
    fn rollback_ops(&mut self, ops_mark: usize, lhh_mark: usize) -> Option<ValueId> {
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        None
    }

    pub(crate) fn try_lower_result_rec_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        
        let rec_ty = self.rec_int_ok_payload(result_ty)?;
        // A RECURSIVE-drop record routes through the generated `$__drop_tup_int_<R>`
        // wrapper; a FLAT (all-scalar-field) record's tuple frees exactly like the
        // (String, Int) tuple — slot0 rc_dec + blocks — so it REUSES DropResultStrInt.
        let drop_fn = self.record_or_anon_drop_type_name(&rec_ty).map(|r| format!("tup_int_{r}"));
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                let IrExprKind::Tuple { elements } = &inner.kind else { return None };
                if elements.len() != 2 {
                    return None;
                }
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                let rec = match self
                    .try_lower_record_construct(&elements[0])
                    .or_else(|| self.try_lower_scalar_record_construct(&elements[0]))
                    .or_else(|| self.lower_result_str_piece(&elements[0]))
                {
                    Some(v) => v,
                    None => return self.rollback_ops(ops_mark, lhh_mark),
                };
                let n = match self.lower_scalar_value(&elements[1]) {
                    Some(v) => v,
                    None => return self.rollback_ops(ops_mark, lhh_mark),
                };
                // The 2-slot tuple block OWNING the record (moved in) + the scalar.
                let two = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: two, value: 2 });
                let tup = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: tup,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::DynList { len: two },
                });
                let th = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
                let rh = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(rh), args: vec![rec] });
                let s0 = self.load_addr(th, 12);
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s0, rh] });
                self.ops.push(Op::Consume { v: rec });
                let s1o = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: s1o, value: 20 });
                let s1 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: s1, op: IntOp::Add, a: th, b: s1o });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s1, n] });
                Some(self.wrap_rec_int_result(tup, repr, false, drop_fn.as_deref()))
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.wrap_rec_int_result(piece, repr, true, drop_fn.as_deref()))
            }
            _ => None,
        }
    }

    /// Is `ty` `Result[(V1, V2), String]` where each pair slot is a registry VARIANT
    /// (RICH — recursive-drop — or FLAT) / String / Bytes, with at least one RICH slot
    /// (#1547 shape 1 — the aggregate-transition return `(new_state, event)`)? Returns
    /// the generated wrapper drop's suffix (`vp_<A>_<B>`). The slot classification
    /// MIRRORS `generate_variant_pair_result_sources`' Finder one for one (the same
    /// record-widened `needs_recursive_drop` both sides of the rich gate use), so an
    /// admission here ALWAYS has a generated `$__drop_vp_<A>_<B>`. An all-flat pair
    /// declines — it frees exactly like `(String, Int)`'s existing flat routes.
    pub(crate) fn variant_pair_result_drop_fn(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return None };
        if a.len() != 2 || !matches!(a[1], Ty::String) {
            return None;
        }
        let Ty::Tuple(ts) = &a[0] else { return None };
        if ts.len() != 2 {
            return None;
        }
        let class = |t: &Ty| -> Option<(String, bool)> {
            match t {
                Ty::Named(n, args) if args.is_empty() => {
                    let ns = n.as_str();
                    if self.variant_layouts.needs_recursive_drop(ns, &|rn| {
                        crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                    }) {
                        Some((ns.to_string(), true))
                    } else if self.variant_layouts.is_flat_variant_ty(t) {
                        Some((ns.to_string(), false))
                    // RECORD slots (#1564 — the matrix's other cell): a
                    // recursive-drop record recurses via `$__drop_<R>`
                    // (record_drop_type_name is the generation mirror), an
                    // all-scalar record is one-level-exact under the flat dec.
                    } else if let Some(rn) = self.record_drop_type_name(t) {
                        Some((rn, true))
                    } else if self
                        .aggregate_field_tys(t)
                        .is_some_and(|(_, tys)| tys.iter().all(|f| !is_heap_ty(f)))
                    {
                        Some((ns.to_string(), false))
                    } else {
                        None
                    }
                }
                Ty::String => Some(("String".to_string(), false)),
                Ty::Bytes => Some(("Bytes".to_string(), false)),
                // LIST slots (#1580 — `(state, List[event])`, the multi-event
                // pair): a SCALAR-element list is one-level-exact (one rc_dec
                // frees the block — no owned inner heap); a `List[String]`
                // slot frees per-element via the vp-private
                // `__drop_vp_list_str` (the shared `__drop_list_str` is gated
                // on record/variant FIELD usage and would dangle here).
                // Deeper element classes (records, variants, nested lists)
                // keep the honest decline. Reserved lowercase names, as with
                // `scalar` below.
                Ty::Applied(TypeConstructorId::List, la) if la.len() == 1 => {
                    if !is_heap_ty(&la[0]) {
                        Some(("list_scalar".to_string(), false))
                    } else if matches!(la[0], Ty::String) {
                        Some(("list_str".to_string(), true))
                    } else {
                        None
                    }
                }
                // A SCALAR slot (`(Int, Note("a", n))` — #1579's mixed pair):
                // stored raw, freed by nothing — the generated drop SKIPS the
                // slot (an rc_dec there would dec a non-handle). The lowercase
                // name is structurally collision-free: type names are
                // Uppercase-initial, so no user type can spell it.
                _ if !is_heap_ty(t) => Some(("scalar".to_string(), false)),
                _ => None,
            }
        };
        let (an, a_rich) = class(&ts[0])?;
        let (bn, b_rich) = class(&ts[1])?;
        (a_rich || b_rich).then(|| {
            format!(
                "vp_{}_{}",
                crate::lower::drop_fn_ident(&an),
                crate::lower::drop_fn_ident(&bn)
            )
        })
    }

    /// Construct a `Result[(V1, V2), String]` `ok((A(..), B(..)))` / `err(<String>)` —
    /// #1547 shape 1, the state-machine transition return `(Done(id), Finished(id))`.
    /// Ok lowers BOTH slots as fresh owned heap values (`lower_owned_heap_field` — a
    /// variant ctor element routes through `try_lower_variant_ctor`), builds the 2-slot
    /// pair block owning them (handles @12 / @20 — the `try_lower_result_rec_int_ctor`
    /// pair build with slot 1's scalar store swapped for a second moved-in handle), and
    /// wraps it via `materialize_result_aggregate`; the wrapper's [`Op::DropWrapperRec`]
    /// recurses via the generated `$__drop_vp_<A>_<B>` (`resrec:vp_<A>_<B>`), and the
    /// Err arm is the flat @12 String the wrapper render already decs. Any inadmissible
    /// piece rolls back and declines — the caller walls (never wrong bytes).
    pub(crate) fn try_lower_result_variant_pair_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};

        let drop_fn = self.variant_pair_result_drop_fn(result_ty)?;
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                let IrExprKind::Tuple { elements } = &inner.kind else { return None };
                if elements.len() != 2 {
                    return None;
                }
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                // Lower both slot values FIRST (before the alloc) so a slot
                // expr that itself allocates does not interleave with the
                // store sequence. A HEAP slot is a fresh owned value moved in;
                // a SCALAR slot (#1579's mixed pair) is a raw value stored
                // directly — no handle, no move, and the generated
                // `$__drop_vp_…` skips its slot.
                let mut slot_vals: Vec<(ValueId, bool)> = Vec::with_capacity(2);
                for e in elements.iter() {
                    if is_heap_ty(&e.ty) {
                        match self.lower_owned_heap_field(e) {
                            Some(v) => slot_vals.push((v, true)),
                            None => return self.rollback_ops(ops_mark, lhh_mark),
                        }
                    } else {
                        match self.lower_scalar_value(e) {
                            Some(v) => slot_vals.push((v, false)),
                            None => return self.rollback_ops(ops_mark, lhh_mark),
                        }
                    }
                }
                // The 2-slot pair block OWNING its heap slots (moved in).
                let two = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: two, value: 2 });
                let tup = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: tup,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::DynList { len: two },
                });
                let th = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
                for (idx, (v, is_heap)) in slot_vals.into_iter().enumerate() {
                    let off = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: off, value: 12 + (idx as i64) * 8 });
                    let slot = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: slot, op: IntOp::Add, a: th, b: off });
                    let store_val = if is_heap {
                        let h = self.fresh_value();
                        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![v] });
                        h
                    } else {
                        v
                    };
                    self.ops.push(Op::Prim {
                        kind: PrimKind::Store { width: 8 },
                        dst: None,
                        args: vec![slot, store_val],
                    });
                    if is_heap {
                        self.ops.push(Op::Consume { v });
                        self.live_heap_handles.retain(|h| *h != v);
                    }
                }
                Some(self.materialize_result_aggregate(tup, repr, false, drop_fn))
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.materialize_result_aggregate(piece, repr, true, drop_fn))
            }
            _ => None,
        }
    }

    /// Construct a `Result[heap-record, String]` `ok(<record>)` / `err(<String>)` — porta
    /// read_valtype's `ok({val, next})`. Ok materializes the owned record (`try_lower_record_construct`,
    /// recursive-drop) and wraps it (the wrapper's [`Op::DropWrapperRec`] recurses via `$__drop_<R>`);
    /// Err wraps a String. `None` outside `Result[<recursive-drop record>, String]` or a
    /// non-materializable payload — so a `Result[String, String]` keeps its existing flat path.
    pub(crate) fn try_lower_result_record_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        // Exactly `Result[<record needing recursive drop>, String]`.
        let ok_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                &a[0]
            }
            _ => return None,
        };
        let drop_fn = self.record_or_anon_drop_type_name(ok_ty)?;
        let repr = repr_of(result_ty).ok()?;
        // Both arms use `lower_result_str_piece` — EXACTLY the payload set the leaky `is_heap_ok_result`
        // path admits (a Record literal routes through its `_ => lower_owned_heap_field` recursive-drop
        // case; an Ok record Var / call / the Err String are handled directly) — so intercepting here
        // un-walls nothing extra and re-walls nothing (no regression), only swapping the flat
        // `DropListStr` for the recursive `Op::DropWrapperRec`.
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.materialize_result_aggregate(piece, repr, false, drop_fn))
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.materialize_result_aggregate(piece, repr, true, drop_fn))
            }
            _ => None,
        }
    }

    /// `ok(<user-variant ctor>)` / `err(<String>)` for `Result[<user variant>, String]` — the derived
    /// variant decode's `ok(Pair(_e0, _e1))` / `ok(Plain)`. Materialize the variant (`try_lower_variant_ctor`
    /// — the SAME tagged block a `let p = Pair(..)` builds, with its recursive-drop set) and wrap it, so
    /// the Ok payload is a REAL variant block the consumer's `match` reads. A RICH variant (a heap field,
    /// e.g. `Pair(Int, String)`) routes the wrapper's drop to the generated `$__drop_<V>` via `resrec:<V>`
    /// ([`Op::DropWrapperRec`]); a FLAT variant frees flat (`DropListStr`). Without this the ctor emitted a
    /// dangling `CallFn "Pair"` (an unlinked call the render wall rejects). `None` outside a variant Ok.
    pub(crate) fn try_lower_result_variant_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let ok_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                &a[0]
            }
            _ => return None,
        };
        let type_name = self.custom_variant_type_name(ok_ty)?;
        let needs_rec = self
            .variant_layouts
            .needs_recursive_drop(&type_name, &|rn| {
                crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
            });
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                // A ctor payload (`ok(Pair(..))`) builds its tagged block; any OTHER heap
                // payload shape — `ok(v)` over a Var holding a variant, `ok(r.node)` over a
                // borrowed projection, a call — lowers to an owned handle via the SAME piece
                // set the record-Result ctor admits (`lower_result_str_piece`: Var → Dup,
                // Member/TupleIndex → borrow-then-Dup, call → fresh CallFn result). Without
                // the fallback a heap→heap payload re-wrap (`let r = g(n)!; ok(r.node)`)
                // walled the whole function — the #1492 headline shape.
                let piece = self
                    .try_lower_variant_ctor(inner)
                    .or_else(|| self.lower_result_str_piece(inner))?;
                // The variant piece is MOVED into the Result @12 (Consumed by the materialize below) and
                // freed by the Result's drop — detach its OWN scope-end drop so it is freed EXACTLY once.
                self.value_drops.get_mut(&piece).map(|d| d.named_route = None);
                self.value_drops.get_mut(&piece).map(|d| d.flat_elems = false);
                if needs_rec {
                    Some(self.materialize_result_aggregate(piece, repr, false, type_name))
                } else {
                    Some(self.materialize_result_str(piece, repr, false, false))
                }
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                if needs_rec {
                    Some(self.materialize_result_aggregate(piece, repr, true, type_name))
                } else {
                    Some(self.materialize_result_str(piece, repr, true, false))
                }
            }
            _ => None,
        }
    }

    /// Is `ty` a `Result[T_scalar, <user variant>]` — the structured-error shape whose
    /// reader seeds LEN-AS-TAG (`seed_variant_param`'s scalar-Ok branch)?
    pub(crate) fn is_scalar_ok_variant_err_result(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2
                && !is_heap_ty(&a[0])
                && self.custom_variant_type_name(&a[1]).is_some())
    }

    /// `err(<user-variant ctor>)` for `Result[T_scalar, <user variant>]` — the
    /// STRUCTURED-ERROR class (`err(Overflow(msg))` / `err(DivZero)`). The reader
    /// (`seed_variant_param`) seeds this type LEN-AS-TAG (Err = len 1 + the payload
    /// HANDLE at slot 0, bound BORROWED by the err arm), so the ctor materializes
    /// exactly that via the len-1 builder (`materialize_opt_str_some` — "Err IS Some
    /// physically"), moving the variant block in. A RICH variant payload
    /// (`Overflow(String)` — its block owns nested heap) routes the wrapper's drop to
    /// the generated `$__drop_res_<V>` (at the wrapper's last ref, an Err recurses
    /// into slot 0 via `$__drop_<V>`); a FLAT payload (`DivZero`) keeps the exact
    /// flat DropListStr. `ok(<scalar>)` for this family keeps the existing scalar-Ok
    /// materializer — the same len-as-tag layout, nothing new. `None` outside the
    /// shape or a non-materializable payload (the sound wall).
    pub(crate) fn try_lower_result_err_variant_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let err_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && !is_heap_ty(&a[0]) =>
            {
                &a[1]
            }
            _ => return None,
        };
        let type_name = self.custom_variant_type_name(err_ty)?;
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::ResultErr { expr: inner } = &expr.kind else {
            return None;
        };
        let piece = self.try_lower_variant_ctor(inner)?;
        let needs_rec = self.variant_layouts.needs_recursive_drop(&type_name, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        });
        // The variant block is MOVED into the Result @slot 0 — detach its own
        // scope-end drop so it frees exactly once, through the wrapper.
        self.value_drops.get_mut(&piece).map(|d| d.named_route = None);
        self.value_drops.get_mut(&piece).map(|d| d.flat_elems = false);
        self.live_heap_handles.retain(|h| *h != piece);
        // The VARIANT-payload Err keeps the classic opt_str_some MOVE (the variant
        // block is moved into slot 0, its own drop route detached above): it is
        // NOT `materialize_result_err_str`'s borrow-contract String shape — a
        // co-owned rich variant would need rc-aware recursive-drop reasoning the
        // move form never needs. Result-only tracking (the value-position
        // both-flags-true conflict is resolved by removing the Option flag).
        let obj = self.materialize_opt_str_some(piece, repr);
        self.value_shapes.remove(&obj);
        self.value_shapes.insert(obj, crate::lower::VariantShape::ResultScalar);
        if needs_rec {
            self.value_drops.get_mut(&obj).map(|d| d.flat_elems = false);
            self.value_drops.entry(obj).or_default().named_route = Some(format!("res_{type_name}"));
        }
        Some(obj)
    }

    /// Is `ty` a `Result[T_scalar, <record>]` — the structured-error shape whose Err is a
    /// RECORD needing a recursive drop (`Result[Int, {code, msg}]`)? The record twin of
    /// [`Self::is_scalar_ok_variant_err_result`], previously an unconditional wall.
    pub(crate) fn is_scalar_ok_rec_err_result(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2
                && !is_heap_ty(&a[0])
                && self.record_or_anon_drop_type_name(&a[1]).is_some())
    }

    /// `err(<record>)` for `Result[T_scalar, <record>]` — the record twin of
    /// [`Self::try_lower_result_err_variant_ctor`]. The reader seeds this type
    /// LEN-AS-TAG (Err = len 1 + the payload handle at slot 0), so the Err block is
    /// the len-1 builder's (`materialize_opt_aggregate_some` — "Err IS Some
    /// physically"), whose `optrec:<R>` route recurses into the record via the
    /// generated `$__drop_<R>` exactly when the wrapper holds a payload (len > 0 =
    /// Err) at its last ref — a flat `DropListStr` would free the record BLOCK and
    /// leak its heap fields. The payload set is `lower_result_str_piece`'s (a
    /// record literal via `lower_owned_heap_field`, a Var → Dup, a call), so both
    /// `err(E { … })` and `err(e)` over a borrowed bind lower. `ok(<scalar>)` for
    /// this family keeps the existing scalar-Ok materializer.
    pub(crate) fn try_lower_result_err_record_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let err_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && !is_heap_ty(&a[0]) =>
            {
                &a[1]
            }
            _ => return None,
        };
        let drop_fn = self.record_or_anon_drop_type_name(err_ty)?;
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::ResultErr { expr: inner } = &expr.kind else {
            return None;
        };
        let piece = self.lower_result_str_piece(inner)?;
        // The record block is MOVED into the Result @slot 0 — detach its own
        // scope-end drop so it frees exactly once, through the wrapper.
        self.value_drops.get_mut(&piece).map(|d| d.named_route = None);
        self.value_drops.get_mut(&piece).map(|d| d.flat_elems = false);
        self.live_heap_handles.retain(|h| *h != piece);
        let obj = self.materialize_opt_aggregate_some(piece, repr, drop_fn);
        self.value_shapes.remove(&obj);
        self.value_shapes.insert(obj, crate::lower::VariantShape::ResultScalar);
        Some(obj)
    }
}
