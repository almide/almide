impl LowerCtx {
    /// Lower ONE arm of a heap-result `if` to the value the arm leaves on the wasm stack.
    /// A string LITERAL is `Alloc{Str}` + `Consume` (the per-arm `"im"` move-out balance —
    /// NOT added to `live_heap_handles`, it is moved out as the result). A NESTED `if` (a
    /// desugared `match`'s else-if) recurses, its result dst being this arm's value.
    fn lower_heap_result_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            // An `e!` arm (`if c then parse_sequence(..)! else ..`) — effect-fn error
            // propagation: `e!` returns e's Result unchanged (Ok→Ok, Err→Err), so strip the
            // `!` and lower `e` as the arm (the same identity the tail-position `e!` uses).
            IrExprKind::Unwrap { expr } => self.lower_heap_result_arm(expr, result_ty),
            // A `??` arm (`(h) => value.as_string(value.get(row,h) ?? …) ?? ""` — the defunc-map cell
            // projection): the unwrap's fresh owned result (a self-hosted unwrap helper / option_str
            // call, cert `i`) + the arm's `Consume` (`m`) = the per-arm `"im"` balance; the operand
            // temp is freed within the arm (`drop_arm_locals`). An out-of-subset `??` returns None →
            // the caller keeps its WALL/defer (no invalid wasm). track_result=false: NOT a scope-end
            // local, it is the moved-out arm value.
            IrExprKind::UnwrapOr { expr, fallback } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_option_unwrap_or(expr, fallback, false)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            IrExprKind::LitStr { value } => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::Str(value.clone()) });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // A bare-Var arm (`if c then a else b` over heap params/locals — the `pick`
            // shape): the arm must MOVE OUT an owned reference, but `a`/`b` are still
            // owned elsewhere (a borrowed param the caller owns, or a let-local with its
            // own scope-end drop). ACQUIRE a fresh reference (`Op::Dup` = cert `i`-grade:
            // a new owned object, rc+1) and move it out (the arm's `Consume` = `m`) — the
            // SAME per-arm `"im"` balance as a literal arm, and the ORIGINAL handle is
            // untouched (no double-free: the Dup'd ref is independent; the original drops
            // exactly once at its own scope end). Sound for BOTH a param (the proven
            // auto-acquire from the tail-Var path) and a tracked local. `value_for` walls
            // an unbound/global var → the caller keeps the Opaque fallback.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.ops.push(Op::Consume { v: dst });
                Some(dst)
            }
            // A string-concat arm (`match x { _ => a + b }`, `if c then a + b else …`) — the
            // __str_concat call's fresh owned String (cert `i`) + the arm's `Consume` (`m`) = the
            // same per-arm `"im"` balance as the call arms; any materialized arg temp is freed
            // within the arm (`drop_arm_locals`). Closes an un-wired concat position (caps recovery).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_concat_str(arm)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A LIST-concat arm (`if string.is_empty(last) then acc else acc + [last]` — the flow_rec
            // base): `__list_concat`/`__list_concat_rc`'s fresh owned list (cert `i`) + the arm's
            // `Consume` (`m`) = the per-arm `"im"` move-out balance. The left operand (`acc`) is BORROWED
            // by the concat (copied), so it is untouched here and freed at its own scope end; any
            // materialized element temp is freed within the arm. Closes the heap-result-`if` return whose
            // arms are an append (the parser-accumulator base case).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_concat_list(arm)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A STRING INTERPOLATION arm (`match e { Click{button,..} => "click:${button}" }`)
            // over the executable subset — the __str_concat chain's fresh owned String (`i`) +
            // the arm's `Consume` (`m`) = the same per-arm `"im"` balance as the concat arm; any
            // intermediate temp is freed within the arm (`drop_arm_locals`). A compound/call-
            // operand interp returns None → the caller keeps the sound Opaque arm fallback. This
            // is REQUIRED for gate exactness: `count_ir_calls` credits a lowerable interp wherever
            // it sits (the visitor walks match/if arms), so the lowering MUST fold it here too,
            // else `ir_calls > mir_calls` falsely taints the function (the −32 caps regression).
            IrExprKind::StringInterp { parts } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_string_interp(parts)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.lower_heap_result_if_inner(cond, then, else_, result_ty)
            }
            // A LIST-literal arm (`if string.is_empty(t) then [] else parse_rows_rec(...)` — the
            // parser entry's empty-or-recurse split): materialize the block + MOVE IT OUT
            // (`Consume` = `m`) — the same per-arm `"im"` as a literal arm. An EMPTY `[]` is a fresh
            // empty list block (no elements to free); a populated heap/scalar list reuses the bind
            // builders (which mark the right recursive-drop set, though the moved-out result is freed
            // by the CALLER per its type, not here).
            IrExprKind::List { elements } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = if elements.is_empty() {
                    let repr = repr_of(result_ty).ok()?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc { dst, repr, init: Init::IntList(vec![]) });
                    dst
                } else {
                    self.try_lower_str_list_literal(arm)
                        .or_else(|| self.try_lower_scalar_list_construct(arm))?
                };
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A TUPLE literal arm (`if c then (a, b) else (0, 0)`, `... else (parse(s), pos)`):
            // materialize the flat (scalar) or nested-ownership (heap-element) tuple block
            // (cert `i`) and MOVE IT OUT (`Consume` = `m`) — the same per-arm `"im"` balance as
            // a literal arm. Any heap element it materializes is freed within the arm.
            IrExprKind::Tuple { elements } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self
                    .try_lower_scalar_tuple_construct(elements)
                    .or_else(|| self.try_lower_tuple_construct(elements))?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A RECORD literal arm (`if len_byte < 0x80 then { tag, length, header_size: 2 } else { … }`
            // — the rsa der_read_tl shape): materialize the record block (scalar-field fast path, else
            // the general nested-ownership construct, cert `i`) and MOVE IT OUT (`Consume` = `m`) — the
            // same per-arm `"im"` balance as the tuple arm. Any heap field it materializes is freed
            // within the arm (`drop_arm_locals`). Unblocks a record returned via a heap-result `if`.
            IrExprKind::Record { .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self
                    .try_lower_scalar_record_construct(arm)
                    .or_else(|| self.try_lower_record_construct(arm))?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A BLOCK arm (`else { let c = string.get(s, pos) ?? ""; <heap-tail> }` — the
            // dominant real-parser shape): lower its statements as effects in a per-arm frame,
            // then its tail as the arm's moved-out heap value (recursing into this same arm
            // lowering, which `Consume`s the tail). The block's own heap let-locals (tracked in
            // `live_heap_handles` since `arm_mark`) are freed WITHIN the arm via
            // `drop_arm_locals`; the moved-out value is `Consume`d (never in that set), so it is
            // not double-freed. Same per-arm balance the scalar block arm proves.
            IrExprKind::Block { stmts, expr } => {
                let tail = expr.as_deref()?;
                let arm_mark = self.live_heap_handles.len();
                self.in_frame += 1;
                let mut ok = true;
                for stmt in stmts {
                    if self.lower_stmt(stmt).is_err() {
                        ok = false;
                        break;
                    }
                }
                let obj = if ok {
                    self.lower_heap_result_arm(tail, result_ty)
                } else {
                    None
                };
                self.drop_arm_locals(arm_mark);
                self.in_frame -= 1;
                obj
            }
            // A direct user-call arm (`if c then f(x) else "d"`): the callee returns a
            // FRESH owned heap value (CallFn-with-heap-result = cert `i`), moved out by the
            // arm's `Consume` (cert `m`) — the same `"im"` balance as a literal arm. Any
            // heap arg the call MATERIALIZES (a heap-literal/fresh-value arg) is dropped
            // WITHIN the arm (`drop_arm_locals`), NOT at function scope: a per-arm temp
            // freed at function scope would `Drop` an uninitialized local when the OTHER arm
            // ran (garbage rc_dec → trap). Per-arm, the temp is freed only if this arm
            // executes — the same per-iteration-balance discipline the loops use.
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                // A DIRECT SELF-RECURSIVE call arm (`name == fn_name`) is the unbounded-
                // stack tail-recursion shape (`fn spin = if … then acc else spin(…)`).
                // v1 has NO TCO, so EXECUTING it deeply overflows the wasm call stack
                // (a fail-stop trap). Executing the heap-result if here would convert a
                // shallow-correct / deep-trapping recursion — a NET LOSS over the sound
                // Opaque fallback for the canonical 2M-deep TCO acceptance fixture. WALL
                // it (→ `None`): the function keeps its memory-safe linearized form until
                // real TCO lands. (A non-self call recurses no deeper than the caller, so
                // it stays admitted.)
                // EXCEPTION: inside a defunctionalized `list.map` body (`children |> list.map((c) =>
                // render_el(c, …))`) the self-call is BOUNDED — it recurses to the tree's DEPTH, not
                // the unbounded linear depth of a tail loop — so executing it is correct (matches v0's
                // own recursion) and is admitted. The wall applies only to a function-TAIL self-call.
                // EXPERIMENT (toml): allow a function-tail self-call to lower as a REAL recursive
                // CallFn (matches v0's own native recursion exactly — same call-stack depth, same
                // bytes). The previous unconditional wall kept a sound Opaque/linearized fallback to
                // avoid a 2M-deep tail-loop wasm stack overflow; but a TCO-able tail loop is already
                // rewritten by try_tco_rewrite BEFORE here (never reaches this arm), so what remains
                // is a general-arg recursion (toml parse_doc/set_nested/append_aot) whose depth is
                // bounded by the input exactly as v0's is. Gated by the full test (the 2M-deep TCO
                // acceptance fixture is TCO'd, not executed here — if it regresses, this is reverted).
                let _ = name;
                let repr = repr_of(result_ty).ok()?;
                let arm_mark = self.live_heap_handles.len();
                let lowered = self.lower_call_args(args).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(obj),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.ops.push(Op::Consume { v: obj });
                // Free materialized arg temps inside the arm (obj is moved out, never in
                // `live_heap_handles`, so it is not among them).
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A PURE stdlib `Module`-call arm (`match n { 0 => "a", _ => int.to_string(n) }` —
            // the single most common real-program shape). Same per-arm `"im"` balance as the
            // Named-call arm: the pure call returns a FRESH owned heap value (`i`), the arm's
            // `Consume` moves it out (`m`); any heap arg it materializes is freed within the arm
            // (`drop_arm_locals`). The purity gate lives in `lower_pure_module_value_call` (an
            // impure/HO/unsupported call errors → `None` → the caller keeps the sound Opaque
            // fallback). Was the gap that dropped a real-program `match → stdlib-call` to Opaque.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, result_ty)
                    .ok()?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A direct Option ctor arm (`if c then Some(x*2) else None` — the filter_map / map
            // closure body): materialize the 0-or-1-element Option block + Consume (move-out)
            // — the SAME per-arm `"im"` balance as a literal arm (init-agnostic `Alloc` = `i`,
            // `Consume` = `m`). `Some`'s payload must be a lowerable scalar (a heap payload
            // aliases its element — a later brick; it falls out of the subset here).
            // A HEAP payload (`Some(string_var)` — an `Option[String]`) materializes a 0-or-1-
            // element `DynListStr` (Machinery 2): the owned String is MOVED into slot 0 (cert `m`)
            // and the whole Option is freed recursively (`DropListStr`) at scope end. Same `Alloc`
            // = `i` + `Consume` = `m` per-arm balance as the scalar case; reuses the proven
            // List[String] cert (init-agnostic). Only a Var payload (the owned slice, let-bound).
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(result_ty).ok()?;
                // The owned String payload: a let-bound Var (its handle), or a direct user-call
                // that RETURNS a fresh owned String (CallFn result, rc 1) — materialized into the
                // Option below (its `Consume` `m` balances the alloc/call `i`).
                let piece = match &expr.kind {
                    // `some(v)` over a Var STILL OWNED elsewhere (a borrowed param, or a local with
                    // its own scope-end drop): `Op::Dup` a fresh owned reference (cert `a`) to MOVE
                    // into the Option, leaving the original to drop once at its scope — never a bare
                    // move-out `m` the checker rejects (param → `am`, owned local → `iamd`).
                    IrExprKind::Var { id } => {
                        let src = self.value_for(*id).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Dup { dst: p, src });
                        p
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
                let obj = self.materialize_opt_str_some(piece, repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::OptionSome { expr } => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::OptSome { payload } });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            // A `None` for an `Option[heap]` is the 0-element `DynListStr` (so `DropListStr` frees
            // it uniformly); a scalar Option keeps `Init::OptNone`.
            IrExprKind::OptionNone if is_heap_elem_list_ty(result_ty) => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.materialize_opt_str_none(repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::OptionNone => {
                let repr = repr_of(result_ty).ok()?;
                let obj = self.fresh_value();
                self.ops.push(Op::Alloc { dst: obj, repr, init: Init::OptNone });
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
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
            // A NESTED `match` arm (`match int.parse(c) { ok(n) => value.int(n), err(_) =>
            // match float.parse(c) { … } }` — try_decimal; `if … then match int.from_hex(..) {
            // ok(n) => value.int(n), err(_) => value.str(raw) } else …` — parse_number's then-arm).
            // Recurse through the SAME machinery the tail-position `match` uses: a variant subject
            // runs the proven `try_lower_variant_value_match` (subject-drop-before-arms over a
            // scalar payload, then a heap-result-`if` skeleton), an Int-literal subject desugars
            // to a nested heap-result `if`. The recursive call ALREADY `Consume`s each leaf arm
            // (the move-out balance) and returns the merged if-result `dst` — so this arm adds NO
            // extra `Consume` (exactly like the nested-`If` arm above), avoiding a double-move-out.
            // Cert-clean: it composes two already-proven, internally-balanced lowerings; on any
            // out-of-subset shape the inner attempt rolls itself back and returns `None`, so the
            // OUTER `try_lower_heap_result_if` restores the op stream and walls the function.
            IrExprKind::Match { subject, arms } => {
                // PER-ARM FRAME: the match SUBJECT (`int.from_hex(string.drop(c, 2))`) materializes
                // heap-arg temps (the `string.drop` result) into `live_heap_handles`. Unlike every
                // other arm kind here, the match lowering does not move them out — they must be freed
                // WITHIN this arm (inside the wasm then/else branch), else they leak to the FUNCTION
                // scope-end where an UNCONDITIONAL `rc_dec` of an uninitialized local (when the OTHER
                // arm ran) is a `rc_dec(0)` trap — the yaml `parse_number` 0x-branch crash. The
                // recursive lowering Consumes the moved-out result (never in the set), so drop_arm_locals
                // frees exactly the subject-eval temps.
                let arm_mark = self.live_heap_handles.len();
                if is_variant_ty(&subject.ty) {
                    if let Some(dst) =
                        self.try_lower_variant_value_match(subject, arms, result_ty)
                    {
                        self.drop_arm_locals(arm_mark);
                        return Some(dst);
                    }
                }
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, result_ty) {
                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                        if let Some(dst) =
                            self.lower_heap_result_if_inner(cond, then, else_, result_ty)
                        {
                            self.drop_arm_locals(arm_mark);
                            return Some(dst);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// `Some(piece)` for `Option[String]` = a 1-element `DynListStr`: store `piece`'s handle into
    /// slot 0 + CONSUME it (moves in), track as nested-ownership list + materialized Option.
    /// Reuses the proven Machinery-2 `store_str` op sequence — no new cert.
    pub(crate) fn materialize_opt_str_some(&mut self, piece: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// `Some((Int, String))` — an `Option[(Int, String)]` as a 1-element list holding the tuple handle
    /// (the `list.find` over a `List[(Int,String)]` result). SAME as `materialize_opt_str_some` but the
    /// payload is a TUPLE, so the Option's drop must RECURSIVELY free it (`$__drop_list_int_str`, the
    /// per-tuple rc==1 guard makes co-ownership with the source list safe) — routed via
    /// `variant_drop_handles="list_int_str"`, NOT the flat `heap_elem_lists` (which would leak the
    /// tuple's String).
    pub(crate) fn materialize_opt_int_str_some(&mut self, piece: ValueId, repr: crate::Repr) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.variant_drop_handles.insert(obj, "list_int_str".to_string());
        self.materialized_options.insert(obj);
        obj
    }

    /// Materialize `None` for an `Option[String]` as a 0-element `DynListStr` (tracked like
    /// `materialize_opt_str_some`). `DropListStr` over len 0 frees only the block.
    pub(crate) fn materialize_opt_str_none(&mut self, repr: crate::Repr) -> ValueId {
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: zero } });
        self.heap_elem_lists.insert(obj);
        self.materialized_options.insert(obj);
        obj
    }

    /// `Ok(string)` / `Err(string)` for a HEAP-Ok `Result[String, String]` = a len-1 `DynListStr`
    /// owning the one String at slot 0 (Ok's value OR Err's message), with the Ok/Err TAG written to
    /// the `cap` field (@8): 0=Ok, 1=Err. `len` stays 1 so `DropListStr` frees the String regardless
    /// of which arm. Cert = `materialize_opt_str_some` (Alloc `i` + the String `m` + scope-end `d`);
    /// the cap-tag store is an opaque prim op. Tracked in `materialized_results_str` for the match.
    /// Is `ty` a `Result[heap, heap]` (e.g. `Result[String, String]`)? Both Ok and Err own a heap
    /// payload, so it uses the cap-as-tag heap-Ok materialization, NOT the scalar len-as-tag one.
    pub(crate) fn is_heap_ok_result(ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
            if a.len() == 2 && is_heap_ty(&a[0]) && is_heap_ty(&a[1]))
    }

    /// Lower a heap-String piece (an `Ok`/`Err` payload) to its owned handle: a tracked Var, a
    /// String literal (fresh Alloc), or a Named-call result. Returns `None` for any other shape.
    pub(crate) fn lower_result_str_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            // `ok(f)` / `err(msg)` over a Var that is STILL OWNED elsewhere — a borrowed param
            // (`fn validate(f) = .. ok(f)`) or a let-local with its own scope-end drop. The piece
            // is MOVED INTO the Result block (`materialize_result_str` `Consume`s it), so it must be
            // a FRESH owned reference: `Op::Dup` (acquire, cert `a`) the var, leaving the original
            // untouched (it drops exactly once at its own scope — no double-free, no bare move-out
            // `m` underflow the proven checker rejects). Same `a…m` balance as the bare-Var if-arm.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                Some(p)
            }
            // `err("missing field '" + key + "'")` — the __str_concat chain's fresh owned String is
            // moved into the Result block (no Dup: it is already a fresh rc=1 reference).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => self.try_lower_concat_str(expr),
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
                Some(p)
            }
            // `ok(string.from_bytes(bytes))` / `ok(int.to_string(n))` — the Ok payload is a stdlib
            // MODULE call (or any other fresh-owned heap producer): lower it as a fresh owned heap
            // value (rc=1) that materialize_result_str then MOVES into the Result block, no Dup. This
            // is base64 decode's `match bs { ok(bytes) => ok(string.from_bytes(bytes)), … }`.
            _ => self.lower_owned_heap_field(expr),
        }
    }

    pub(crate) fn materialize_result_str(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        is_err: bool,
        value_ok: bool,
    ) -> ValueId {
        use crate::PrimKind;
        // A 1-SLOT DynListStr (cap 1, len 1 — IDENTICAL block size to every other String/Value block,
        // so the single-head free-list reuses it; a wider block would be a distinct size that the
        // size-exact reuse leaks). Slot 0's LOW 32 bits (@12) own the String handle, its HIGH 32 bits
        // (@16) carry the Ok/Err tag — `DropListStr` does `i32.wrap` of the i64 slot, taking ONLY the
        // low-32 handle to free, so the high-32 tag is inert (never mistaken for a handle).
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        // slot 0 LOW (@12) := the String handle (zero-extended i64 → high 32 bits cleared), CONSUME
        // the piece (move-in). This 8-byte store MUST precede the tag store (it zeroes @16).
        let off12 = self.const_add(oh, 12);
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![off12, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        // slot 0 HIGH (@16) := the Ok/Err tag (0 = Ok, 1 = Err) — overwrites the cleared high half.
        let off16 = self.const_add(oh, 16);
        let tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tag, value: if is_err { 1 } else { 0 } });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![off16, tag] });
        // A Value-Ok Result (`Result[Value, String]`) drops via the recursive `Op::DropResultValue`
        // (Ok → `$__drop_value`); a String-Ok Result via the flat `DropListStr` (rc_dec the String).
        if value_ok {
            self.value_result_results.insert(obj);
        } else {
            self.heap_elem_lists.insert(obj);
        }
        self.materialized_results_str.insert(obj);
        obj
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

    /// Try to lower `for i in start..end { body }` over a SCALAR Int index as a REAL loop
    /// that EXECUTES every step — desugaring the range to the same while machinery
    /// (`LoopStart`/`LoopBreakUnless`/`LoopEnd` + `SetLocal`). The index is its own stable
    /// local initialized to `start` and incremented by 1 each iteration; `end` is snapshot
    /// ONCE before the loop (v0 builds the range once). Restricted to the runnable subset:
    /// a LITERAL `start` (so the index local is a fresh, distinct `ConstInt` — safe to
    /// mutate, never aliasing a caller value), a scalar-lowerable `end`, an Int loop var
    /// (no tuple), no `break`/`continue`, and a heap-reassign-free body (the
    /// `scalar_loop_depth` rule errors otherwise). Returns false (rolled back) when out of
    /// subset; `lower_for_in` then falls back to its sound model-one-iteration form.
    pub(crate) fn try_lower_scalar_for_range(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        let IrExprKind::Range { start, end, inclusive } = &iterable.kind else {
            return false;
        };
        if var_tuple.is_some()
            || body_breaks_or_continues(body)
            || matches!(find_var_ty(body, var), Some(t) if !matches!(t, Ty::Int))
            || !matches!(start.kind, IrExprKind::LitInt { .. })
        {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Snapshot `end` once; init the index local `i = start` (a fresh ConstInt — a
        // distinct, mutable local, never aliasing a caller value). `one` for the step.
        let end_v = match self.lower_scalar_value(end) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                return false;
            }
        };
        if self.lower_bind(var, &Ty::Int, start).is_err() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        let Some(&i_v) = self.value_of.get(&var) else {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        };
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        // The bound test, re-read each iteration: `i < end` (exclusive) / `i <= end` (incl).
        let cond_v = self.fresh_value();
        let cmp = if *inclusive { IntOp::Le } else { IntOp::Lt };
        self.ops.push(Op::IntBinOp { dst: cond_v, op: cmp, a: i_v, b: end_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        // The implicit step `i = i + 1`, then the back-edge.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }

    /// EXECUTE `for x in xs { … }` over a `List[T]` as a real loop (vs the model-one-iteration
    /// form): borrow the list handle once, walk an internal index `i` 0..len via the loop markers,
    /// bind element `i` to the loop var `x` each iteration, run the body.
    ///
    /// TWO element shapes, BOTH borrowing the list (read-only; the list keeps owning its elements):
    /// - a SCALAR element (`List[Int/Float/Bool]`, i64 slots) — `Load { width: 8 }` the slot and
    ///   `SetLocal` the loop var (a stable mutable i64 local, a COPY, no ownership);
    /// - a HEAP element (`List[String]` / nested-ownership DynListStr, i32-handle slots) — the loop
    ///   var is the BORROWED element handle, `LoadHandle`d fresh each iteration into `value_of[var]`
    ///   and recorded in `param_values` so it is NOT a second owner (the list's recursive drop frees
    ///   the element; the loop var must not free it — no double-free). The body reads the element via
    ///   string/list ops; a body that MOVES the element out (stores it elsewhere) is not in this
    ///   subset (the borrow stays read-only), so such a body rolls back.
    ///
    /// SOUND by reuse of the for-range / while machinery: the body is per-iteration-balanced
    /// (`drop_arm_locals`), the markers no-op in the cert (it verifies ONE balanced iteration), the
    /// `i < len` guard runs the body the REAL number of times (0 for an empty list — closing the
    /// model-one-iteration bug that ran a heap-element body ONCE on a garbage handle). GATED to a
    /// `List[scalar]` / heap-element list, a matching loop-var type, no tuple/break/continue.
    pub(crate) fn try_lower_scalar_for_list(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        use crate::PrimKind;
        // The element type: a scalar `List[Int/Float/Bool]` (i64 slot) OR a heap-element list
        // (`List[String]`, i32-handle slot). A Map / non-list iterable defers.
        let elem_ty = match &iterable.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return false,
        };
        let elem_heap = is_heap_ty(&elem_ty);
        // A heap-AGGREGATE element (tuple/record) read via a direct FIELD/INDEX projection
        // (`for p in ps { p.0 }`) projects off the WRONG handle here — a silent miscompile. Decline
        // ONLY that projecting case (`body_reads_var_field`) so it falls to lower_for_in, which WALLs
        // it (honest). A `let (x, y) = p` destructure (tuple PATTERN) or passing `p` whole is loaded
        // correctly by this real per-element loop, so it is NOT declined (no regression).
        if elem_heap
            && self.aggregate_field_tys(&elem_ty).is_some()
            && var_tuple.is_none()
            && body_reads_var_field(body, var)
        {
            return false;
        }
        // The element SHAPE (scalar vs heap) comes from the iterable's element type, so the loop var
        // is bound correctly even when it is UNUSED in the body (an `for _ in xs`, or a loop kept for
        // its effect count) — `find_var_ty` returns None then, which must NOT fall to the model-one-
        // iteration form (that ran the body ONCE; an empty list must run it ZERO times). When the var
        // IS used, its body-declared type must agree with the element shape (a defensive consistency
        // gate against a mis-typed body).
        let var_ty = find_var_ty(body, var);
        if let Some(vt) = &var_ty {
            if is_heap_ty(vt) != elem_heap {
                return false;
            }
        }
        if var_tuple.is_some() || body_breaks_or_continues(body) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // Borrow the list (evaluated once); a Var is borrowed, a fresh literal is materialized
        // (owned, dropped at the outer scope — it stays in live_heap_handles). A heap-element
        // list LITERAL (`for s in ["x", "y"]`) needs its elements actually stored, so route it
        // through `try_lower_str_list_literal` (the filled owned list) rather than the generic
        // `lower_call_args` Alloc path (which would leave an empty/opaque block → zero iterations).
        let str_list_literal =
            elem_heap && matches!(&iterable.kind, IrExprKind::List { elements } if !elements.is_empty());
        let list_v = if str_list_literal {
            match self.try_lower_str_list_literal(iterable) {
                Some(v) => {
                    self.live_heap_handles.push(v);
                    v
                }
                None => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        } else {
            match self.lower_call_args(std::slice::from_ref(iterable)) {
                Ok(args) => match args.into_iter().next() {
                    Some(CallArg::Handle(v)) => v,
                    _ => {
                        self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                        return false;
                    }
                },
                Err(_) => {
                    self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                    return false;
                }
            }
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        // The SCALAR loop var is a stable mutable i64 local, `SetLocal` to element[i] each iteration.
        // (A HEAP loop var is bound fresh per iteration below — no stable local: a borrowed i32
        // handle re-`LoadHandle`d inside the loop.)
        let x_v = if elem_heap {
            None
        } else {
            let x = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: x, value: 0 });
            self.value_of.insert(var, x);
            Some(x)
        };

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });
        // The element-slot address `h + 12 + i*8`.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: i8_v });
        if let Some(x_v) = x_v {
            // Scalar element: x = load64(slot) — a COPY into the stable mutable local.
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![addr] });
            self.ops.push(Op::SetLocal { local: x_v, src: elem });
        } else {
            // Heap element: x = the BORROWED i32 handle at the slot (LoadHandle, Ptr repr), bound
            // fresh each iteration. Recorded in `param_values` — the list still OWNS the element
            // (its recursive DropListStr frees it), so the loop var is NOT a second owner and is
            // NOT added to the per-iteration drop set (no double-free).
            let elem = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(elem), args: vec![addr] });
            self.value_of.insert(var, elem);
            self.param_values.insert(elem);
        }

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;
        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        self.drop_arm_locals(body_mark);
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);
        true
    }
}
