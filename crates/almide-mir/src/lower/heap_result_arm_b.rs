impl LowerCtx {

    /// Every `ResultOk`/`ResultErr` shape guard (Value / (String,Int) / (record,Int) /
    /// (Value,Int) / (List[String],Int) / (List[Value],Int) / record / structured-variant-
    /// error / custom-variant / Option[record] / Option[scalar|String] / plain heap String /
    /// Unit-Ok / heap Err). Guard order is load-bearing (most specific result-shape first,
    /// generic heap-String fallback last) — every pattern and every guard is kept verbatim
    /// and in the ORIGINAL order.
    /// Split (codopsy round-3 sweep, #852): every arm BODY moved wholesale into the named
    /// `..._arm` decider it calls and every compound guard into its named predicate, so this
    /// match is now a pure ROUTER over the Value-Result + `(payload, Int)` tuple-Ok family;
    /// its `_` arm hands the REST of the chain to
    /// [`Self::lower_heap_result_record_variant_option_arm`], which hands its own tail to
    /// [`Self::lower_heap_result_str_payload_arm`]. Both are pure SUFFIX delegates and so
    /// cannot change which arm commits: a delegate is reached only once every earlier
    /// pattern+guard has already failed, exactly as in the original single match — the
    /// commit-on-first-true-guard semantics are preserved exactly. An arm whose BODY declines
    /// (a `?` → `None`) still returns `None` for the whole chain, never falling through to a
    /// later arm, likewise as before.
    fn lower_heap_result_arm_result(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_value_result_ty(result_ty) =>
            {
                self.lower_value_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_str_int_result_ty(result_ty) =>
            {
                self.lower_str_int_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_rec_int_result_ty(result_ty) =>
            {
                self.lower_rec_int_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.variant_pair_result_drop_fn(result_ty).is_some() =>
            {
                self.lower_variant_pair_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_value_int_result_ty(result_ty) =>
            {
                self.lower_value_int_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_list_str_int_result_ty(result_ty) =>
            {
                self.lower_list_str_int_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_list_value_int_result_ty(result_ty) =>
            {
                self.lower_list_value_int_result_ctor_arm(arm, result_ty)
            }
            _ => self.lower_heap_result_record_variant_option_arm(arm, result_ty),
        }
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the dynamic-`Value`-Ok result-ctor arm body.
    ///
    /// `Ok(int)` / `Err(string)` arms of a `Result[Int, String]`-returning heap `if` (the
    /// parse-family shape `if ok then Ok(v) else Err("msg")`). Result reuses the Option[String]
    /// DynListStr layout with len-AS-TAG: `Ok` = a cap-1/len-0 block (the int sits in slot 0
    /// but DropListStr frees no element — like `None`); `Err` = a cap-1/len-1 block owning the
    /// message String (DropListStr frees it — exactly `Some(string)`). So BOTH arms reuse the
    /// proven Option[String] cert (Alloc `i` + the per-arm `Consume` `m`; the Err's String is
    /// moved in `m` and freed by the scope-end DropListStr `d`) — NO new Init, NO checker change.
    /// `Result[Value, String]` (the `ok(value.array(...))` shape — csv `parse`): the Ok payload
    /// is a dynamic Value (materialized via `lower_owned_heap_field`, which handles the
    /// `value.*` ctor + the nested `list.map`), the Err a String. Same len-1 + tag@16 block, but
    /// marked `value_result_results` so the drop is the RECURSIVE `Op::DropResultValue` (Ok →
    /// `$__drop_value`). Checked BEFORE the String-Ok arm (Value is also a heap-ok result).
    fn lower_value_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_value_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `(String, Int)`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[(String, Int), String]` (toml parse_key_part's `ok((slice, end))` AS A
    /// HEAP-RESULT-IF/MATCH ARM, not just the tail): reuse the brick-1 producer
    /// try_lower_result_str_int_ctor + its recursive DropResultStrInt drop. Checked BEFORE the
    /// generic heap-Ok String arm (which would route a (String,Int) tuple Ok through a flat
    /// DropListStr, leaking the tuple's String). Same per-arm frame as the Value-Result arm.
    fn lower_str_int_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_str_int_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `(record, Int)`-Ok result-ctor arm body.
    ///
    /// `ok((GGUFHeader {…}, 24))` / err — a (record, Int) tuple Ok (gguf parse_header).
    fn lower_rec_int_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_rec_int_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// The `(V1, V2)` VARIANT-PAIR result-ctor arm body (#1547 shape 1):
    /// `ok((Done(id), Finished(id)))` / `err("already")` arms of a
    /// `Result[(St, Ev), String]`-returning match — the state-machine transition
    /// return. Same per-arm frame as every sibling (`arm_mark` / ctor / `Consume` /
    /// `drop_arm_locals`); the ctor's recursive drop is `resrec:vp_<A>_<B>`.
    fn lower_variant_pair_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_variant_pair_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `(Value, Int)`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[(Value, Int), String]` (toml parse_val's `ok((value.…, pos))` as an
    /// if/match arm) — the (Value,Int) tuple counterpart, recursive DropResultValueInt.
    fn lower_value_int_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_value_int_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `(List[String], Int)`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[(List[String], Int), String]` (toml parse_key/parse_table_key as an
    /// if/match arm) — the (List[String],Int) tuple counterpart, recursive DropResultListStrInt.
    fn lower_list_str_int_result_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_list_str_int_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `(List[Value], Int)`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[(List[Value], Int), String]` (toml collect_array_items as an if/match arm).
    fn lower_list_value_int_result_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_list_value_int_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_result`] (codopsy round-3 sweep, #852):
    /// the middle stretch of that router's guard chain — the heap-RECORD Ok, the structured-
    /// variant Err (scalar-Ok and, further down the chain, heap-Ok), the custom-variant Ok and
    /// the two `Option`-Ok result shapes. A pure SUFFIX delegate of the router: it is reached
    /// only when every earlier pattern+guard has failed, so relocating these arms cannot change
    /// which arm commits. Every pattern and every guard is verbatim and in the ORIGINAL order;
    /// the tail beyond them moves on again to [`Self::lower_heap_result_str_payload_arm`].
    fn lower_heap_result_record_variant_option_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_record_result_ty(result_ty) =>
            {
                self.lower_record_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_variant_err_result(result_ty) => {
                self.lower_scalar_ok_variant_err_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_rec_err_result(result_ty) => {
                self.lower_scalar_ok_rec_err_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_custom_variant_ok_payload_ty(result_ty) =>
            {
                self.lower_custom_variant_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_option_record_result_ty(result_ty) =>
            {
                self.lower_option_record_result_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_option_scalar_str_result_ty(result_ty) =>
            {
                self.lower_option_scalar_str_result_ctor_arm(arm, result_ty)
            }
            _ => self.lower_heap_result_str_payload_arm(arm, result_ty),
        }
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the heap-RECORD-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[heap-record, String]` (porta read_valtype's `ok({val, next})`): the Ok
    /// payload is a heap RECORD, the Err a String. Checked BEFORE the generic heap-Ok String arm
    /// (which routes a record Ok through a flat `DropListStr`, leaking the record's nested heap
    /// fields). `try_lower_result_record_ctor` wraps the materialized record (Ok) / String (Err)
    /// and routes the wrapper's drop to the recursive `$__drop_<R>` (`Op::DropWrapperRec`). Same
    /// per-arm frame as the Value-Result arm. Guard = `Result[<recursive-drop record>, String]`,
    /// so a `Result[String, String]` keeps its existing path below.
    fn lower_record_result_ctor_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_record_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the scalar-Ok structured-variant-Err ctor arm body.
    ///
    /// SCALAR-Ok `Result[T_scalar, <user variant>]` ERR arm (the structured-error
    /// class: `err(Overflow(msg))`, `err(DivZero)` — bidirectional_type): the
    /// reader seeds this type LEN-AS-TAG, so materialize the variant payload into
    /// the len-1 wrapper; a rich payload routes the drop to `$__drop_res_<V>`.
    fn lower_scalar_ok_variant_err_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_err_variant_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// The scalar-Ok RECORD-Err ctor arm body — the record twin of
    /// [`Self::lower_scalar_ok_variant_err_ctor_arm`], same per-arm `"im"` frame.
    fn lower_scalar_ok_rec_err_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_err_record_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the custom-variant-Ok arm's guard — the `Result[_, String]`'s Ok
    /// payload (or, for a bare non-`Result` type, the type itself) names a CUSTOM variant.
    fn is_custom_variant_ok_payload_ty(&self, result_ty: &Ty) -> bool {
        self.custom_variant_type_name(match result_ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                &a[0]
            }
            _ => result_ty,
        })
        .is_some()
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the custom-variant-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[<user variant>, String]` (derived variant decode's `ok(Pair(..))` /
    /// `ok(Plain)` if/match arms): materialize the variant Ok / String Err, recursive `$__drop_<V>`.
    fn lower_custom_variant_result_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_variant_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `Option[record]`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[Option[record], String]` (read_message's `ok(none)` / `ok(r)` arms): the
    /// Ok payload is an `Option[record]`, freed recursively via the generated `$__drop_opt_<R>`
    /// (`resrec:opt_<R>`) — NOT the flat `DropListStr` that would leak the Some record. Guard =
    /// `Result[Option[<recursive-drop record>], String]`; `Result[Option[String], String]` keeps
    /// the flat path below.
    fn lower_option_record_result_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_option_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the `Option[String|scalar]`-Ok result-ctor arm body.
    ///
    /// HEAP-Ok `Result[Option[T], String]` with a STRING / SCALAR leaf (the derived-Codec
    /// `__decode_option_T` if/match arms — `ok(some(x))` / `ok(none)` / `err(e)`): a scalar Option
    /// frees flat (`DropListStr`), a String Option recursively (`$__drop_opt_str`). Checked AFTER
    /// the record-Option arm (disjoint by leaf), BEFORE the generic heap-Ok String arm.
    fn lower_option_scalar_str_result_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_option_scalar_str_ctor(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_result`] (codopsy round-3 sweep, #852):
    /// the TAIL of that router's guard chain — the generic heap-String Ok/Err arms (whose
    /// len-as-tag block cannot distinguish Ok from Err by length, so the tag lives in cap@8),
    /// the heap-Ok structured-variant Err ctor that must precede them, the Unit/scalar-payload
    /// Ok, and the final `Err(message)`-as-`Some(message)` arm. A pure SUFFIX delegate of
    /// [`Self::lower_heap_result_record_variant_option_arm`] (itself a suffix delegate of the
    /// router): reached only when every earlier pattern+guard has failed, so it cannot change
    /// which arm commits. Every pattern and every guard is verbatim and in the ORIGINAL order,
    /// ending on the router chain's original `_ => None`.
    fn lower_heap_result_str_payload_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::ResultOk { expr }
                if Self::is_heap_payload_of_heap_ok_result(&expr.ty, result_ty) =>
            {
                self.lower_heap_ok_str_wrap_arm(expr, result_ty)
            }
            IrExprKind::ResultErr { expr }
                if self.is_heap_err_variant_ctor_payload(expr, result_ty) =>
            {
                self.lower_heap_ok_variant_err_ctor_arm(arm, result_ty)
            }
            IrExprKind::ResultErr { expr }
                if Self::is_heap_payload_of_heap_ok_result(&expr.ty, result_ty) =>
            {
                self.lower_heap_err_str_wrap_arm(expr, result_ty)
            }
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                self.lower_scalar_or_unit_ok_wrap_arm(expr, result_ty)
            }
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                self.lower_err_message_opt_str_wrap_arm(expr, result_ty)
            }
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the guard SHARED by the generic heap-String `Ok` and `Err` arms — the
    /// arm's payload is a heap value and the result is a heap-Ok `Result`.
    fn is_heap_payload_of_heap_ok_result(payload_ty: &Ty, result_ty: &Ty) -> bool {
        is_heap_ty(payload_ty) && Self::is_heap_ok_result(result_ty)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the heap-Ok structured-variant-Err arm's guard — a heap Err payload
    /// that is NOT a bare `Var` (a `err(<Var>)` payload keeps the generic route below) in a
    /// `Result[H, <user variant>]`.
    fn is_heap_err_variant_ctor_payload(&self, expr: &IrExpr, result_ty: &Ty) -> bool {
        is_heap_ty(&expr.ty)
            && !matches!(&expr.kind, IrExprKind::Var { .. })
            && self.is_heap_ok_variant_err_result(result_ty)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the generic heap-String `Ok` arm body.
    ///
    /// HEAP-Ok `Result[String, String]`: BOTH `Ok(string)` and `Err(string)` own a String, so
    /// len-as-tag can't distinguish — materialize a len-1 DynListStr + the Ok/Err tag in cap@8.
    fn lower_heap_ok_str_wrap_arm(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        // FRAME the payload-build temps: a `${…}`/concat Ok payload (`ok("ok" +
        // int.to_string(k))`) materializes intermediate concat Strings (`lower_result_str_piece`
        // pushes them to `live_heap_handles`) that must be freed WITHIN this arm; the final
        // `piece` is MOVED into the Ok block (Consume — not dropped). WITHOUT the per-arm frame
        // those temps escaped to `emit_scope_end_drops`, emitting an UNCONDITIONAL post-join
        // `rc_dec` that ran on the NOT-TAKEN (err) arm where the temp local is 0 → the `$rc_dec`
        // double-free sentinel `unreachable` trap. Mirrors the sibling Err arm below.
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let piece = self.lower_result_str_piece(expr)?;
        let obj = self.materialize_result_str(piece, repr, false, false);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the heap-Ok structured-variant-Err ctor arm body.
    ///
    /// HEAP-Ok `Result[H, <user variant>]` ERR CTOR arm (the heap-Ok structured-error
    /// class — classify's `err(NegativeInput(x))` in `Result[String, MathError]`):
    /// MUST precede the generic both-heap Err arm below, whose
    /// `lower_result_str_piece` Named-call fallback emitted the ctor as a dangling
    /// `(call $NegativeInput)` (unlinked at render). A `err(<Var>)` payload keeps
    /// the generic route (the ctor helper only takes ctor shapes).
    fn lower_heap_ok_variant_err_ctor_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_lower_result_err_variant_ctor_heap_ok(arm, result_ty)?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the generic heap-String `Err` arm body — the sibling of
    /// [`Self::lower_heap_ok_str_wrap_arm`], same cap@8 tag layout with `is_err = true`.
    fn lower_heap_err_str_wrap_arm(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        // Same per-arm frame as the Ok arm above (and the non-heap-ok Err arm below): free the
        // Err message-build intermediate temps within the arm; the final `piece` is moved in.
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let piece = self.lower_result_str_piece(expr)?;
        let obj = self.materialize_result_str(piece, repr, true, false);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the scalar-or-Unit-payload `Ok` arm body.
    fn lower_scalar_or_unit_ok_wrap_arm(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        // `ok(())` — a Result[Unit, String] Ok with a UNIT payload (porta `validate`/`stop`:
        // `if cond then err(msg) else ok(())`). Unit has no value, so lower_scalar_value declines
        // it; use a 0 placeholder — the Ok tag (@4 = 0) is what consumers read, the payload @12 is
        // never extracted for a Unit Ok. Without this the whole heap-result `if` walled.
        //
        // PER-ARM FRAME (the dn4 rc_dec(0) trap, 2026-07-12): a scalar payload expr can
        // still materialize HEAP TEMPS — `ok(list.get(date, 0) ?? 0 + …)` builds the
        // `??`-operand Option in live_heap_handles. Leaked to the FUNCTION scope end,
        // its unconditional rc_dec reads an UNINITIALIZED local when the OTHER arm ran
        // (the yaml parse_number class). Frame + drop them WITHIN the arm.
        let arm_mark = self.live_heap_handles.len();
        let payload = if matches!(&expr.kind, IrExprKind::Unit) {
            let z = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: z, value: 0 });
            z
        } else if expr.ty == Ty::Unit {
            // A Unit payload with EFFECTS — `ok(println(x))`, the shape the
            // guard restructure leaves for `guard c else err(…)` in an
            // `effect fn -> Unit` (#1734). The payload IS the statement: run
            // its effects in this arm, then the 0 placeholder exactly as the
            // literal-Unit arm above (the payload slot is never extracted).
            let ops_mark = self.ops.len();
            if self.lower_stmt_expr(expr).is_err() {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(arm_mark);
                return None;
            }
            let z = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: z, value: 0 });
            z
        } else {
            self.lower_scalar_value(expr)?
        };
        let repr = repr_of(result_ty).ok()?;
        let obj = self.materialize_result_ok(payload, repr);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the trailing heap-`Err` arm body — the message is wrapped as the
    /// physically identical `Some(message)` block.
    fn lower_err_message_opt_str_wrap_arm(
        &mut self,
        expr: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let repr = repr_of(result_ty).ok()?;
        // Frame the message-build temps: a `${…}` interpolation (`err("bad char '${ch}'")` —
        // base64 char_to_val) materializes intermediate concat Strings that must be freed
        // WITHIN this arm; the final message `piece` is MOVED into the Err block (not dropped).
        let arm_mark = self.live_heap_handles.len();
        let piece = self.lower_result_err_message_piece(expr)?;
        // `Err` block via `materialize_result_err_str` (semantic init, MOVE
        // contract): the piece is consumed into slot 0 and detached from
        // live_heap_handles INSIDE the materializer, so the per-arm teardown
        // frees only the interpolation's intermediates, never the moved message.
        let obj = self.materialize_result_err_str(piece, repr);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted verbatim from [`Self::lower_heap_result_arm_result`] (codopsy round-3
    /// sweep, #852): the trailing heap-`Err` arm's inner payload match — the owned message
    /// String the `Some(message)` block is built around. Its `_ => return None` declines the
    /// whole arm exactly as it did inside the original single match (the caller's `?`).
    fn lower_result_err_message_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let piece = match &expr.kind {
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                // A BORROWED payload (a heap-Err match bind — slot-0 LoadHandle in
                // `param_values`, owned by the subject that drops AFTER the arms): acquire a
                // fresh owned reference (`Op::Dup`) so re-wrapping it into the Err block does
                // NOT double-free when the subject's `DropListStr` frees slot-0. A plain owned
                // local (`err(msg)` over a let-bound String) is moved in as before — no Dup.
                if self.param_values.contains(&src) {
                    let p = self.fresh_value();
                    self.ops.push(Op::Dup { dst: p, src });
                    p
                } else {
                    src
                }
            }
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                p
            }
            // `err("…${x}…")` — a string interpolation message: fold it to the __str_concat
            // chain (a fresh owned String), exactly like the StringInterp value arm above.
            IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts)?,
            // `err("failed: " + path + ": " + e)` — an explicit `+` concat message (the
            // ggml load shape; borrowed payload vars Dup inside the concat machinery).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                self.try_lower_concat_str(expr)?
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
        Some(piece)
    }

}
