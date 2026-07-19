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

    pub(crate) fn try_lower_result_rec_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        let rec_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                match &a[0] {
                    Ty::Tuple(ts)
                        if ts.len() == 2
                            && matches!(ts[1], Ty::Int)
                            && self.aggregate_field_tys(&ts[0]).is_some()
                            && !matches!(ts[0], Ty::String) =>
                    {
                        ts[0].clone()
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };
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
                    None => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                };
                let n = match self.lower_scalar_value(&elements[1]) {
                    Some(v) => v,
                    None => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
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
                Some(match &drop_fn {
                    Some(df) => self.materialize_result_aggregate(tup, repr, false, df.clone()),
                    None => {
                        let obj = self.materialize_result_str(tup, repr, false, false);
                        self.heap_elem_lists.remove(&obj);
                        self.str_int_result_results.insert(obj);
                        obj
                    }
                })
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(match &drop_fn {
                    Some(df) => self.materialize_result_aggregate(piece, repr, true, df.clone()),
                    None => {
                        let obj = self.materialize_result_str(piece, repr, true, false);
                        self.heap_elem_lists.remove(&obj);
                        self.str_int_result_results.insert(obj);
                        obj
                    }
                })
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
                let piece = self.try_lower_variant_ctor(inner)?;
                // The variant piece is MOVED into the Result @12 (Consumed by the materialize below) and
                // freed by the Result's drop — detach its OWN scope-end drop so it is freed EXACTLY once.
                self.variant_drop_handles.remove(&piece);
                self.heap_elem_lists.remove(&piece);
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
        self.variant_drop_handles.remove(&piece);
        self.heap_elem_lists.remove(&piece);
        self.live_heap_handles.retain(|h| *h != piece);
        let obj = self.materialize_opt_str_some(piece, repr);
        // `materialize_opt_str_some` is the SHARED builder genuine Option construction also
        // uses — it only ever marks `materialized_options`. This object is conceptually a
        // RESULT (an `err(<variant ctor>)`), so ALSO track it in `materialized_results` —
        // scoped to just this call site (not the shared builder, which stays Option-only for
        // its other callers). `try_lower_variant_value_match` (the value-position twin) already
        // resolves the both-flags-true conflict in favor of Result; this closes the same gap
        // for its statement-position sibling `try_lower_result_match`, which had no Option
        // fallback at all and always saw this subject as untracked.
        self.materialized_results.insert(obj);
        if needs_rec {
            self.heap_elem_lists.remove(&obj);
            self.variant_drop_handles.insert(obj, format!("res_{type_name}"));
        }
        Some(obj)
    }

    /// Is `ty` a `Result[T_heap, <user variant>]` — the HEAP-Ok structured-error shape
    /// (`classify: Result[String, MathError]`)? Cap-as-tag (both arms heap), so the
    /// reader is `seed_variant_param`'s both-heap branch and the eq is
    /// `result_eq_general_from_handles`'s tag@16 route.
    pub(crate) fn is_heap_ok_variant_err_result(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2
                && is_heap_ty(&a[0])
                && self.custom_variant_type_name(&a[1]).is_some())
    }

    /// `err(<user-variant ctor>)` for `Result[T_heap, <user variant>]` — the HEAP-Ok
    /// twin of [`Self::try_lower_result_err_variant_ctor`]. The variant ctor is INLINED
    /// (`try_lower_variant_ctor` — a ctor is NOT a wasm fn; the generic heap-Err arm's
    /// `lower_result_str_piece` Named-call fallback emitted a dangling `(call $NegativeInput)`)
    /// and MOVED into the CAP-AS-TAG wrapper (`materialize_result_str` — payload @12,
    /// tag @16 = 1), the exact layout the both-heap reader + `result_eq_general` read.
    /// A RICH variant type (Overflow(String) — nested heap) routes the wrapper's drop to
    /// the ERR-side recursion (`reserr:<V>` → `DropWrapperRec` `err_rec`: tag@16 == 1 →
    /// `$__drop_<V>`, Ok → flat `rc_dec` of the @12 String); a FLAT variant keeps the
    /// flat `DropListStr` (its block owns no nested heap). `None` outside the shape.
    pub(crate) fn try_lower_result_err_variant_ctor_heap_ok(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let err_ty = match result_ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && is_heap_ty(&a[0]) =>
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
        // The variant block is MOVED into the wrapper @12 — detach its own scope-end
        // drop so it frees exactly once, through the wrapper.
        self.variant_drop_handles.remove(&piece);
        self.heap_elem_lists.remove(&piece);
        self.live_heap_handles.retain(|h| *h != piece);
        let obj = self.materialize_result_str(piece, repr, true, false);
        if needs_rec {
            self.heap_elem_lists.remove(&obj);
            self.variant_drop_handles.insert(obj, format!("reserr:{type_name}"));
        }
        Some(obj)
    }

    /// `ok(<Option[R] value>)` / `ok(none)` / `err(<String>)` for `Result[Option[R], String]` where R is
    /// a record needing a recursive drop — read_message's `ok(none)` / `ok(r)` bases (r:
    /// `Option[JsonRpcRequest]`). The Ok payload (an Option Var → `Dup`; `some(record)` / `none` →
    /// materialized) is MOVED into the Result block @12; the wrapper's drop routes to `$__drop_opt_<R>`
    /// via `resrec:opt_<R>` ([`Op::DropWrapperRec`], certificate UNIFORM over `drop_fn` — no Coq change).
    /// `$__drop_opt_<R>` is GENERATED (`generate_record_drop_sources`) as `fn __drop_opt_<R>(e: Option[R])
    /// = match e { some(r) => (), none => () }` (frees the record via `$__drop_<R>` + the Option block).
    /// `None` outside `Result[Option[<recursive-drop record>], String]` or a non-materializable payload.
    pub(crate) fn try_lower_result_option_ctor(
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
        let rec = match ok_ty {
            Ty::Applied(TypeConstructorId::Option, oa) if oa.len() == 1 => &oa[0],
            _ => return None,
        };
        let rec_drop = self.record_or_anon_drop_type_name(rec)?;
        let drop_fn = format!("opt_{rec_drop}");
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                let piece = self.lower_option_piece(inner, rec)?;
                Some(self.materialize_result_aggregate(piece, repr, false, drop_fn))
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.materialize_result_aggregate(piece, repr, true, drop_fn))
            }
            _ => None,
        }
    }

    /// `ok(some(x))` / `ok(none)` / `err(msg)` RETURNED for a `Result[Option[T], String]` whose Option
    /// payload is a STRING or a SCALAR leaf (Int/Float/Bool) — the derived-Codec `__decode_option_T`
    /// shape (`Result[Option[Int], String]` … `Result[Option[String], String]`). The record/tuple/value
    /// Option payloads are handled by [`Self::try_lower_result_option_ctor`] (recursive `$__drop_opt_<R>`)
    /// and MUST be left to it — this helper declines them.
    ///
    /// The Ok payload is the 0-or-1 Option block (`try_lower_option_ctor` — a scalar `Init::OptSome` or a
    /// String-holding `DynListStr`), MOVED into the Result @12. The DROP differs by leaf:
    ///   • SCALAR leaf — the Option[scalar] block owns no inner heap, so the FLAT `materialize_result_str`
    ///     (`heap_elem_lists` → `DropListStr` `rc_dec`s @12) frees it fully, exactly like a `Result[String,
    ///     String]`. No generated drop fn.
    ///   • STRING leaf — the Option[String] block owns the inner String, so a flat `rc_dec` of @12 would
    ///     LEAK it. Route through `materialize_result_aggregate` with `resrec:opt_str` → the generated
    ///     `$__drop_opt_str(e: Option[String])` (emitted by `generate_record_drop_sources`), whose
    ///     `match e { some(r) => (), none => () }` drops the inner String at the some-arm end.
    pub(crate) fn try_lower_result_option_scalar_str_ctor(
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
        let leaf = match ok_ty {
            Ty::Applied(TypeConstructorId::Option, oa) if oa.len() == 1 => &oa[0],
            _ => return None,
        };
        let is_str = matches!(leaf, Ty::String);
        let is_scalar = matches!(leaf, Ty::Int | Ty::Float | Ty::Bool);
        if !is_str && !is_scalar {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                if is_str {
                    let opt_repr = repr_of(ok_ty).ok()?;
                    // Build the `Option[String]` block DIRECTLY: `some(<string>)` co-owns its payload by
                    // `lower_owned_heap_field` (a Dup for a borrowed param / match-ok String, an Alloc for
                    // a literal, a move for a call) — `try_lower_option_ctor` declines a borrowed-Var
                    // payload. `none` is a 0-element block.
                    let piece = match &inner.kind {
                        IrExprKind::OptionSome { expr: payload } => {
                            let s = self.lower_owned_heap_field(payload)?;
                            self.materialize_opt_str_some(s, opt_repr)
                        }
                        IrExprKind::OptionNone => self.materialize_opt_str_none(opt_repr),
                        _ => return None,
                    };
                    // `materialize_opt_str_some`/`_none` mark the block for a flat scope-end `DropListStr`
                    // (`heap_elem_lists`). It is MOVED into the Result @12 (Consumed) and freed by the
                    // Result's `resrec:opt_str` → `$__drop_opt_str` instead — detach it so it is freed
                    // EXACTLY once (no double-free).
                    self.heap_elem_lists.remove(&piece);
                    Some(self.materialize_result_aggregate(piece, repr, false, "opt_str".to_string()))
                } else {
                    let piece = self.try_lower_option_ctor(inner, ok_ty)?;
                    Some(self.materialize_result_str(piece, repr, false, false))
                }
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                if is_str {
                    Some(self.materialize_result_aggregate(piece, repr, true, "opt_str".to_string()))
                } else {
                    Some(self.materialize_result_str(piece, repr, true, false))
                }
            }
            _ => None,
        }
    }

    /// Build the `Option[R]` Ok payload for [`try_lower_result_option_ctor`]: an Option Var (`Dup` a
    /// fresh owned ref), `some(record)` (materialize the record into the 0-or-1 Option block via
    /// `materialize_opt_aggregate_some`), or `none` (a 0-element Option block). `None` otherwise.
    fn lower_option_piece(&mut self, inner: &IrExpr, rec: &Ty) -> Option<ValueId> {
        match &inner.kind {
            // `ok(r)` where r is an owned/borrowed `Option[R]` local — `Dup` a fresh owned reference
            // (the original drops once at its scope; the Result's @12 owns the Dup'd one).
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::OptionNone => {
                // A 0-element Option block (no record inside) — the same empty 0-or-1 layout the
                // some-builder emits, so `$__drop_opt_<R>` frees it uniformly (its `match` takes none).
                let repr = repr_of(&inner.ty).ok()?;
                let z = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: z, value: 0 });
                let obj = self.fresh_value();
                self.ops
                    .push(Op::Alloc { dst: obj, repr, init: crate::Init::DynListStr { len: z } });
                self.variant_drop_handles.insert(obj, format!("opt_{}", self.record_or_anon_drop_type_name(rec)?));
                Some(obj)
            }
            IrExprKind::OptionSome { expr: rec_expr } => {
                let repr = repr_of(&inner.ty).ok()?;
                let piece = self.lower_result_str_piece(rec_expr)?;
                let drop_fn = self.record_or_anon_drop_type_name(rec)?;
                Some(self.materialize_opt_aggregate_some(piece, repr, drop_fn))
            }
            _ => None,
        }
    }

    /// Construct a `Result[<non-heap>, String]` `ok(<scalar/unit>)` block — the porta
    /// `run_foreground` / `ensure_porta_dir` `ok(())` tail and any `ok(<Int/Bool>)`. The Ok payload
    /// is a SCALAR (or Unit → a `0` placeholder; the @4 len-0 field is the Ok tag consumers read, the
    /// @12 payload slot is never extracted for a Unit Ok), wrapped by `materialize_result_ok` into the
    /// flat len-0 block (scope-end `DropListStr` frees just the block — no nested heap to recurse).
    /// Returns the block (NOT Consumed — the caller moves it out as a tail return, or pushes
    /// `Op::Consume` for a heap-result-if/match arm). `None` outside `Result[<non-heap>, String]`, a
    /// non-`ResultOk`, or a HEAP Ok payload — those route to the heap-ok / record / value ctors above.
    pub(crate) fn try_lower_result_scalar_ok_ctor(
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
        if is_heap_ty(ok_ty) {
            return None;
        }
        let IrExprKind::ResultOk { expr: inner } = &expr.kind else {
            return None;
        };
        if is_heap_ty(&inner.ty) {
            return None;
        }
        let payload = if matches!(&inner.kind, IrExprKind::Unit) {
            let z = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: z, value: 0 });
            z
        } else {
            self.lower_scalar_value(inner)?
        };
        let repr = repr_of(result_ty).ok()?;
        Some(self.materialize_result_ok(payload, repr))
    }

    /// Construct a `Result[Value, String]` `ok(<Value>)` / `err(<String>)` (the `ok(value.array(...))`
    /// shape) — the len-1 + tag@16 block, Ok payload a Value (materialized via `lower_owned_heap_field`,
    /// which handles the `value.*` ctor + nested `list.map`), Err a String. Marked
    /// `value_result_results` so the drop is the recursive `Op::DropResultValue`. Returns the block
    /// (NOT yet Consumed — the caller moves it out as a tail return or an arm `Consume`). `None` for a
    /// non-`Result[Value, String]` type or a payload outside the materializable subset.
    pub(crate) fn try_lower_result_value_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !crate::lower::is_value_result_ty(result_ty) {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => {
                let piece = self.lower_owned_heap_field(inner)?;
                Some(self.materialize_result_str(piece, repr, false, true))
            }
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                Some(self.materialize_result_str(piece, repr, true, true))
            }
            _ => None,
        }
    }

    /// Construct a `Result[(String, Int), String]` `ok((<String>, <Int>))` / `err(<String>)` — the
    /// toml `parse_key_part` `ok((slice, pos))` shape. Ok materializes the `(String, Int)` tuple
    /// (`try_lower_tuple_construct`, rc-owning the String slot) and wraps it in the cap-as-tag block
    /// (payload @12 = the tuple handle); Err wraps a String. Tracked in `str_int_result_results` so the
    /// scope-end drop is the recursive [`Op::DropResultStrInt`] (frees the tuple's String + both blocks)
    /// — NOT the flat `heap_elem_lists`/`DropListStr` `materialize_result_str` defaults to, which would
    /// leak the tuple's String. Returns the wrapper block (moved out as the tail return), or `None`
    /// outside the exact `Result[(String, Int), String]` / materializable-payload subset.
    pub(crate) fn try_lower_result_str_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let is_str_int = matches!(result_ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2
                && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                    && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Int))
                && matches!(a[1], Ty::String));
        if !is_str_int {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        let obj = match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => match &inner.kind {
                IrExprKind::Tuple { elements } => {
                    let tup = self.try_lower_tuple_construct(elements)?;
                    self.materialize_result_str(tup, repr, false, false)
                }
                _ => return None,
            },
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                self.materialize_result_str(piece, repr, true, false)
            }
            _ => return None,
        };
        // Re-route the drop: materialize_result_str(value_ok=false) tracked `heap_elem_lists`
        // (flat DropListStr); a (String, Int)-tuple Ok needs the recursive DropResultStrInt.
        self.heap_elem_lists.remove(&obj);
        self.str_int_result_results.insert(obj);
        Some(obj)
    }

    /// Construct a `Result[(Value, Int), String]` `ok((<Value>, <Int>))` / `err(<String>)` — the toml
    /// `parse_val` shape. Identical to `try_lower_result_str_int_ctor` except the Ok-tuple's slot0 is a
    /// dynamic `Value` (so the scope-end drop is the recursive `Op::DropResultValueInt` via
    /// `$__drop_value_tuple`, tracked in `value_int_result_results`). `None` outside the exact
    /// `Result[(Value, Int), String]` / materializable-payload subset.
    pub(crate) fn try_lower_result_value_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !crate::lower::is_value_int_result_ty(result_ty) {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        let obj = match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => match &inner.kind {
                IrExprKind::Tuple { elements } => {
                    let tup = self.try_lower_tuple_construct(elements)?;
                    self.materialize_result_str(tup, repr, false, false)
                }
                _ => return None,
            },
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                self.materialize_result_str(piece, repr, true, false)
            }
            _ => return None,
        };
        self.heap_elem_lists.remove(&obj);
        self.value_int_result_results.insert(obj);
        Some(obj)
    }

    /// Construct a `Result[(List[Value], Int), String]` `ok((<List[Value]>, <Int>))` / `err(<String>)`
    /// — toml `collect_array_items`. The Ok-tuple's slot0 is a `List[Value]`, so the scope-end drop is
    /// the recursive `Op::DropResultListValueInt` (`$__drop_list_value_tuple`), tracked in
    /// `list_value_int_result_results`. `None` outside the exact type / materializable-payload subset.
    pub(crate) fn try_lower_result_list_value_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !crate::lower::is_list_value_int_result_ty(result_ty) {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        let obj = match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => match &inner.kind {
                IrExprKind::Tuple { elements } => {
                    let tup = self.try_lower_tuple_construct(elements)?;
                    self.materialize_result_str(tup, repr, false, false)
                }
                _ => return None,
            },
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                self.materialize_result_str(piece, repr, true, false)
            }
            _ => return None,
        };
        self.heap_elem_lists.remove(&obj);
        self.list_value_int_result_results.insert(obj);
        Some(obj)
    }

    /// Construct a `Result[(List[String], Int), String]` `ok((<List[String]>, <Int>))` / `err(<String>)`
    /// — the toml `parse_key` / `parse_table_key` shape. The Ok-tuple's slot0 is a `List[String]`, so
    /// the scope-end drop is the recursive `Op::DropResultListStrInt` (frees each element String + the
    /// List block + the tuple block), tracked in `list_str_int_result_results`. `None` outside the exact
    /// `Result[(List[String], Int), String]` / materializable-payload subset.
    pub(crate) fn try_lower_result_list_str_int_ctor(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !crate::lower::is_list_str_int_result_ty(result_ty) {
            return None;
        }
        let repr = repr_of(result_ty).ok()?;
        let obj = match &expr.kind {
            IrExprKind::ResultOk { expr: inner } => match &inner.kind {
                IrExprKind::Tuple { elements } => {
                    let tup = self.try_lower_tuple_construct(elements)?;
                    self.materialize_result_str(tup, repr, false, false)
                }
                _ => return None,
            },
            IrExprKind::ResultErr { expr: inner } => {
                let piece = self.lower_result_str_piece(inner)?;
                self.materialize_result_str(piece, repr, true, false)
            }
            _ => return None,
        };
        self.heap_elem_lists.remove(&obj);
        self.list_str_int_result_results.insert(obj);
        Some(obj)
    }

    /// `handle + k` as a fresh i64 address value (ConstInt + IntBinOp::Add).
    fn const_add(&mut self, base: ValueId, k: i64) -> ValueId {
        let c = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: c, value: k });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::Add, a: base, b: c });
        dst
    }

    /// `Ok(int)` for `Result[Int, String]` = a cap-1/len-0 `DynListStr`: allocate ONE element slot
    /// (so the block is the same physical size as an `Err`'s, free-list-compatible via cap), store
    /// the int in slot 0, then OVERRIDE the len field to 0 so `DropListStr` frees no element (the
    /// int is scalar, owns nothing). Cert: a `None`-like DynListStr (Alloc `i`, no String move-in,
    /// scope-end DropListStr `d`) — the int store + len override are opaque prim ops the checker
    /// ignores. The tag read (len == 0) marks it `Ok`.
    /// `Err(<scalar>)` for a SCALAR-SCALAR Result (`Result[Int, Int]` — the
    /// match_container `ck(err(404))` class): the SAME len-as-tag block as
    /// [`Self::materialize_result_ok`] but the len field STAYS 1 (the Err tag) and slot 0
    /// holds the SCALAR err payload. Deliberately NOT `heap_elem_lists`-tracked: a
    /// DropListStr over len 1 would rc_dec the raw scalar as a handle (the rc_dec-trap
    /// class); the caller's flat `Op::Drop` frees the block exactly (neither arm owns
    /// children). Cert: one Alloc `i` + the scope-end `d` — the same balanced pair.
    pub(crate) fn materialize_result_err_scalar(
        &mut self,
        payload: ValueId,
        repr: crate::Repr,
    ) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynList { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 (handle + 12) = the scalar Err payload; len stays 1 = the Err tag.
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let daddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: daddr, op: IntOp::Add, a: oh, b: twelve });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 8 },
            dst: None,
            args: vec![daddr, payload],
        });
        obj
    }

    pub(crate) fn materialize_result_ok(&mut self, payload: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 (handle + 12) = the Ok int.
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let daddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: daddr, op: IntOp::Add, a: oh, b: twelve });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![daddr, payload] });
        // len field (handle + 4) := 0 so DropListStr treats it as element-free (the Ok tag).
        let four = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: four, value: 4 });
        let laddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: laddr, op: IntOp::Add, a: oh, b: four });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![laddr, zero] });
        self.heap_elem_lists.insert(obj);
        obj
    }
}
