impl LowerCtx {

    /// Every `ResultOk`/`ResultErr` shape guard (Value / (String,Int) / (record,Int) /
    /// (Value,Int) / (List[String],Int) / (List[Value],Int) / record / structured-variant-
    /// error / custom-variant / Option[record] / Option[scalar|String] / plain heap String /
    /// Unit-Ok / heap Err). Guard order is load-bearing (most specific result-shape first,
    /// generic heap-String fallback last) — kept as ONE atomic match, verbatim, so the
    /// original commit-on-first-true-guard semantics are preserved exactly.
    fn lower_heap_result_arm_result(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            // `Ok(int)` / `Err(string)` arms of a `Result[Int, String]`-returning heap `if` (the
            // parse-family shape `if ok then Ok(v) else Err("msg")`). Result reuses the Option[String]
            // DynListStr layout with len-AS-TAG: `Ok` = a cap-1/len-0 block (the int sits in slot 0
            // but DropListStr frees no element — like `None`); `Err` = a cap-1/len-1 block owning the
            // message String (DropListStr frees it — exactly `Some(string)`). So BOTH arms reuse the
            // proven Option[String] cert (Alloc `i` + the per-arm `Consume` `m`; the Err's String is
            // moved in `m` and freed by the scope-end DropListStr `d`) — NO new Init, NO checker change.
            // `Result[Value, String]` (the `ok(value.array(...))` shape — csv `parse`): the Ok payload
            // is a dynamic Value (materialized via `lower_owned_heap_field`, which handles the
            // `value.*` ctor + the nested `list.map`), the Err a String. Same len-1 + tag@16 block, but
            // marked `value_result_results` so the drop is the RECURSIVE `Op::DropResultValue` (Ok →
            // `$__drop_value`). Checked BEFORE the String-Ok arm (Value is also a heap-ok result).
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_value_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_value_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[(String, Int), String]` (toml parse_key_part's `ok((slice, end))` AS A
            // HEAP-RESULT-IF/MATCH ARM, not just the tail): reuse the brick-1 producer
            // try_lower_result_str_int_ctor + its recursive DropResultStrInt drop. Checked BEFORE the
            // generic heap-Ok String arm (which would route a (String,Int) tuple Ok through a flat
            // DropListStr, leaking the tuple's String). Same per-arm frame as the Value-Result arm.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_str_int_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_str_int_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // `ok((GGUFHeader {…}, 24))` / err — a (record, Int) tuple Ok (gguf parse_header).
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_rec_int_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_rec_int_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[(Value, Int), String]` (toml parse_val's `ok((value.…, pos))` as an
            // if/match arm) — the (Value,Int) tuple counterpart, recursive DropResultValueInt.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_value_int_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_value_int_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[(List[String], Int), String]` (toml parse_key/parse_table_key as an
            // if/match arm) — the (List[String],Int) tuple counterpart, recursive DropResultListStrInt.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_list_str_int_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_list_str_int_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[(List[Value], Int), String]` (toml collect_array_items as an if/match arm).
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if crate::lower::is_list_value_int_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_list_value_int_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[heap-record, String]` (porta read_valtype's `ok({val, next})`): the Ok
            // payload is a heap RECORD, the Err a String. Checked BEFORE the generic heap-Ok String arm
            // (which routes a record Ok through a flat `DropListStr`, leaking the record's nested heap
            // fields). `try_lower_result_record_ctor` wraps the materialized record (Ok) / String (Err)
            // and routes the wrapper's drop to the recursive `$__drop_<R>` (`Op::DropWrapperRec`). Same
            // per-arm frame as the Value-Result arm. Guard = `Result[<recursive-drop record>, String]`,
            // so a `Result[String, String]` keeps its existing path below.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_record_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_record_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // SCALAR-Ok `Result[T_scalar, <user variant>]` ERR arm (the structured-error
            // class: `err(Overflow(msg))`, `err(DivZero)` — bidirectional_type): the
            // reader seeds this type LEN-AS-TAG, so materialize the variant payload into
            // the len-1 wrapper; a rich payload routes the drop to `$__drop_res_<V>`.
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_variant_err_result(result_ty) => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_err_variant_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[<user variant>, String]` (derived variant decode's `ok(Pair(..))` /
            // `ok(Plain)` if/match arms): materialize the variant Ok / String Err, recursive `$__drop_<V>`.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self
                    .custom_variant_type_name(match result_ty {
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                            if a.len() == 2 && matches!(a[1], Ty::String) =>
                        {
                            &a[0]
                        }
                        _ => result_ty,
                    })
                    .is_some() =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_variant_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[Option[record], String]` (read_message's `ok(none)` / `ok(r)` arms): the
            // Ok payload is an `Option[record]`, freed recursively via the generated `$__drop_opt_<R>`
            // (`resrec:opt_<R>`) — NOT the flat `DropListStr` that would leak the Some record. Guard =
            // `Result[Option[<recursive-drop record>], String]`; `Result[Option[String], String]` keeps
            // the flat path below.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_option_record_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_option_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[Option[T], String]` with a STRING / SCALAR leaf (the derived-Codec
            // `__decode_option_T` if/match arms — `ok(some(x))` / `ok(none)` / `err(e)`): a scalar Option
            // frees flat (`DropListStr`), a String Option recursively (`$__drop_opt_str`). Checked AFTER
            // the record-Option arm (disjoint by leaf), BEFORE the generic heap-Ok String arm.
            IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                if self.is_option_scalar_str_result_ty(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_option_scalar_str_ctor(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // HEAP-Ok `Result[String, String]`: BOTH `Ok(string)` and `Err(string)` own a String, so
            // len-as-tag can't distinguish — materialize a len-1 DynListStr + the Ok/Err tag in cap@8.
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(result_ty) =>
            {
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
            // HEAP-Ok `Result[H, <user variant>]` ERR CTOR arm (the heap-Ok structured-error
            // class — classify's `err(NegativeInput(x))` in `Result[String, MathError]`):
            // MUST precede the generic both-heap Err arm below, whose
            // `lower_result_str_piece` Named-call fallback emitted the ctor as a dangling
            // `(call $NegativeInput)` (unlinked at render). A `err(<Var>)` payload keeps
            // the generic route (the ctor helper only takes ctor shapes).
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty)
                    && !matches!(&expr.kind, IrExprKind::Var { .. })
                    && self.is_heap_ok_variant_err_result(result_ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_result_err_variant_ctor_heap_ok(arm, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(result_ty) =>
            {
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
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
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
                } else {
                    self.lower_scalar_value(expr)?
                };
                let repr = repr_of(result_ty).ok()?;
                let obj = self.materialize_result_ok(payload, repr);
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(result_ty).ok()?;
                // Frame the message-build temps: a `${…}` interpolation (`err("bad char '${ch}'")` —
                // base64 char_to_val) materializes intermediate concat Strings that must be freed
                // WITHIN this arm; the final message `piece` is MOVED into the Err block (not dropped).
                let arm_mark = self.live_heap_handles.len();
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
                // `Err` IS `Some(message)` physically (cap-1/len-1 DynListStr owning the String):
                // `piece` is MOVED into slot 0 (removed from live_heap_handles), so the per-arm
                // teardown frees only the interpolation's intermediates, never the moved-in message.
                self.live_heap_handles.retain(|h| *h != piece);
                let obj = self.materialize_opt_str_some(piece, repr);
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            _ => None,
        }
    }

}
