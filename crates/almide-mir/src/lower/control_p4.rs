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
            // A SPREAD-record arm (`match arg { "--" => { ...opts, wasm_args: list.drop(args, i) } }`
            // — the porta `parse_options` terminal arm): materialize the fresh same-layout block
            // (`try_lower_spread_record_construct` — non-overridden fields copied from the
            // materialized base, overrides stored) and MOVE IT OUT (`Consume` = `m`) — the same
            // per-arm `"im"` balance as the Record arm. The producer registers the block's
            // `record_masks` so the moved-out value is freed by the CALLER per its type (not here);
            // any transient override temp is freed within the arm (`drop_arm_locals`). A
            // non-materialized base / out-of-subset override returns None → the caller keeps its
            // sound Opaque/wall.
            IrExprKind::SpreadRecord { .. } => {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_spread_record_construct(arm)?;
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
            // A heap-result MATCH arm — the monadic `!`-desugar inside a tail-duplicated
            // `if` (`let xs = if c then load(p)! else []; ok(xs + t)` becomes
            // `if c then { match load(p) { err(e)=>err(e), ok(xs)=>ok(xs+t) } } else …`,
            // porta resolve_env/serve/validate). Delegate to the SAME variant value-match
            // machinery the fn-tail position already uses (rollback-safe: a shape outside
            // its subset returns None and the caller keeps the wall — never invalid wasm).
            IrExprKind::Match { subject, arms }
                if crate::lower::is_variant_ty(&subject.ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_lower_variant_value_match(subject, arms, result_ty)?;
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
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
            // A `some(<record>)` arm — Option wrapping a heap RECORD (porta find_eq_pos's
            // `some({key: key, val: val})`). Materialize the owned record payload
            // (`try_lower_record_construct`, recursive-drop), wrap it in the 0-or-1 Option, and route
            // the Option's scope-end drop to the recursive `$__drop_<R>` (`Op::DropWrapperRec`) so the
            // record's nested heap fields are freed — NOT the flat `DropListStr` that leaks them. Same
            // per-arm `"im"` balance (Alloc `i` + the move-out `Consume` `m`); the record-construct's
            // transient temps are freed within the arm (`drop_arm_locals`). Gated on the record needing
            // a recursive drop (`record_or_anon_drop_type_name`) — a scalar-only record has no
            // `$__drop_<R>` and is not reached here (it would fall through to the deferred path).
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                let arm_mark = self.live_heap_handles.len();
                let repr = repr_of(result_ty).ok()?;
                let drop_fn = self.record_or_anon_drop_type_name(&expr.ty)?;
                let piece = self.try_lower_record_construct(expr)?;
                let obj = self.materialize_opt_aggregate_some(piece, repr, drop_fn);
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
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
                    // `some(string.slice(s, …))` — a PURE Module call yielding a fresh owned
                    // String payload (the parse_tag tail-`if` family): the self-host call's
                    // result moves into the Option (retain-removed — the Option is the sole
                    // owner); its arg temps free within the arm frame below.
                    IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self
                            .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty)
                            .ok()?;
                        self.live_heap_handles.retain(|h| *h != p);
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
            // A heap-result `Computed`-callee call arm (`xs |> list.map((p) => param_ty(p))` — the
            // bindgen inner-map cell calls a let-bound INLINE lambda returning String). C1 HEAP
            // DIRECT-CALL INLINE: defunctionalize it to its inlined body — a FRESH OWNED heap value,
            // moved out by this arm's `Consume` (cert `m`), the same per-arm `"im"` balance as the
            // Named-call arm. The inline tracks its result in `live_heap_handles`; detach it (it is
            // moved out, not a scope-end local) before `Consume`, then `drop_arm_locals` frees any
            // arg/body temp the inline left. A non-let-lambda callee rolls back (`None`) → the caller
            // keeps its sound Opaque/wall (no invalid wasm).
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if is_heap_ty(&arm.ty) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let obj = self.try_inline_direct_lambda_call_heap(callee, args, result_ty)?;
                // The inlined result is moved out of this arm (Consume), so detach it from the live
                // set; `drop_arm_locals` then frees only the inline's transient temps.
                self.live_heap_handles.retain(|h| *h != obj);
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                Some(obj)
            }
            // A bare heap-FIELD-projection arm (`preopen_dirs: if … then opts.preopen_dirs else […]`
            // — the porta build_config spread-override If's then-arm is `opts.preopen_dirs`, a
            // `Member`). The arm must MOVE OUT an owned reference, but the field is still owned by its
            // container (`opts`, a borrowed param the caller owns). BORROW the slot handle
            // (`LoadHandle` of `container_handle + offset`) and ACQUIRE a fresh owned reference
            // (`dup_borrowed_slot` = `Op::Dup`, cert `a`-grade), then MOVE it out (`Op::Consume` =
            // cert `m`) — the SAME per-arm `"am"` balance as the bare-Var arm, with the ORIGINAL
            // slot untouched (no double-free: the Dup'd ref is independent; the container drops its
            // own ref once at its scope end). A `TupleIndex` projection is identical.
            // `dup_borrowed_slot` tracks the owned ref in `live_heap_handles`; the `retain` detaches
            // it (it is moved out, NOT a scope-end local) before the per-arm teardown. Defers (`None`)
            // for an unresolvable container / non-heap slot — the caller keeps its sound wall.
            //
            // SCOPED to a BORROWED-PARAM container (`is_borrowed_param_container` — `opts` is a record
            // param the CALLER owns): this is the RETURN-materializer brick for projecting a borrowed
            // param's heap field. A LOCAL container (`else result.out` over a `list.fold` result, the
            // playground `wrap_lists`) is the LOOP-CARRIED-accumulator frontier (the `(B)` mechanism) —
            // admitting it makes the enclosing fold body lower, whose defunctionalized elided-call
            // count then outruns the source count-gate (a caps WALL BREACH). Defer the local-container
            // case (`None`) so it keeps its existing wall — the loop-slot work owns it. The param case
            // is exactly the documented borrow-then-`Dup` `dup_borrowed_slot` is built for.
            IrExprKind::Member { object, field } if self.is_borrowed_param_container(object) => {
                let offset = self.aggregate_field_offset_any(&object.ty, field.as_str())?;
                let arm_mark = self.live_heap_handles.len();
                let h = self.resolve_aggregate_container_handle(object)?;
                let owned = self.dup_borrowed_slot(h, offset);
                self.ops.push(Op::Consume { v: owned });
                self.live_heap_handles.retain(|x| *x != owned);
                self.drop_arm_locals(arm_mark);
                Some(owned)
            }
            IrExprKind::TupleIndex { object, index } if self.is_borrowed_param_container(object) => {
                let offset = self.aggregate_index_offset_any(&object.ty, *index)?;
                let arm_mark = self.live_heap_handles.len();
                let h = self.resolve_aggregate_container_handle(object)?;
                let owned = self.dup_borrowed_slot(h, offset);
                self.ops.push(Op::Consume { v: owned });
                self.live_heap_handles.retain(|x| *x != owned);
                self.drop_arm_locals(arm_mark);
                Some(owned)
            }
            _ => None,
        }
    }

    /// Is `container` a direct reference to a BORROWED heap PARAM (a record/tuple param the caller
    /// owns — its handle is in `param_values`)? Gates the heap-FIELD-projection return-materializer
    /// arm to exactly the build_config `opts.preopen_dirs` shape, excluding a fold/loop-derived LOCAL
    /// container (the `(B)` loop-accumulator frontier) whose lowering breaches the elided-call count
    /// gate. A non-Var / unbound / non-param container is NOT borrowed-param → `false` (defer).
    pub(crate) fn is_borrowed_param_container(&self, container: &IrExpr) -> bool {
        match &container.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .map(|v| self.param_values.contains(&v))
                .unwrap_or(false),
            _ => false,
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

    /// `Some(<record>)` — an `Option[heap-record]` as a 1-element list holding the record handle @12
    /// (`some({key, val})` — porta find_eq_pos). SAME block as `materialize_opt_str_some` (the record
    /// is MOVED in at slot 0), but the Option's drop must RECURSIVELY free the record's nested heap
    /// fields — route it to `$__drop_<drop_fn>` via `variant_drop_handles="optrec:<drop_fn>"` (→
    /// [`Op::DropWrapperRec`] `is_result=false`), NOT the flat `heap_elem_lists`/`DropListStr` (which
    /// would `rc_dec` the record HANDLE only, leaking its String/List/Value fields). Cert is identical
    /// (Alloc `i` + the record `m` + the scope-end recursive `d`); only the drop ROUTE differs.
    pub(crate) fn materialize_opt_aggregate_some(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        drop_fn: String,
    ) -> ValueId {
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
        self.variant_drop_handles.insert(obj, format!("optrec:{drop_fn}"));
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
    /// Is `ty` a `Result[<record needing recursive drop>, String]` (porta read_valtype's
    /// `Result[{val, next}, String]`)? Gates the record-Ok Result ctor (arm + tail) — distinct from
    /// `is_heap_ok_result` (which would route a record Ok through the leaky flat `DropListStr`).
    pub(crate) fn is_record_result_ty(&self, ty: &Ty) -> bool {
        self.result_ok_record_drop_fn(ty).is_some()
    }

    /// A `Result[Option[<record needing recursive drop>], String]` — read_message's
    /// `Result[Option[JsonRpcRequest], String]`. Its `ok(none)` / `ok(some(rec))` / `ok(<Option Var>)`
    /// arms route to `try_lower_result_option_ctor` (the recursive `$__drop_opt_<R>`). `None` for a
    /// `Result[Option[String], String]` (flat) or a non-Option Ok.
    pub(crate) fn is_option_record_result_ty(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(&a[1], Ty::String) {
            return false;
        }
        let Ty::Applied(TypeConstructorId::Option, oa) = &a[0] else { return false };
        oa.len() == 1 && self.record_or_anon_drop_type_name(&oa[0]).is_some()
    }

    /// A `Result[Option[T], String]` whose Option leaf is a STRING or a SCALAR (Int/Float/Bool) — the
    /// derived-Codec `__decode_option_T` shape. Gates the `try_lower_result_option_scalar_str_ctor` arm
    /// (if/match) and is DISJOINT from `is_option_record_result_ty` (a record leaf) — the two together
    /// cover every `Result[Option[<leaf>], String]` the executable subset admits.
    pub(crate) fn is_option_scalar_str_result_ty(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Result, a) = ty else { return false };
        if a.len() != 2 || !matches!(&a[1], Ty::String) {
            return false;
        }
        let Ty::Applied(TypeConstructorId::Option, oa) = &a[0] else { return false };
        oa.len() == 1 && matches!(&oa[0], Ty::String | Ty::Int | Ty::Float | Ty::Bool)
    }

    /// For a `Result[<record needing recursive drop>, String]` (`Result[Manifest, String]` —
    /// porta load_manifest / resolve_run_caps), the Ok record's generated recursive-drop name
    /// `<R>` (→ `$__drop_<R>`, registered by `build_record_layouts` / synthesized for an anon
    /// record). This is the SINGLE shape gate shared by BOTH the record-Result CONSTRUCTION
    /// (`try_lower_result_record_ctor` → `materialize_result_aggregate`, `resrec:<R>`) and the
    /// record-Result MATCH-SUBJECT path (`try_lower_variant_value_match`): the subject's
    /// scope-end drop routes through `Op::DropWrapperRec { is_result: true }` (recurse into the
    /// @12 record via `$__drop_<R>` at the Ok tag, else `rc_dec` the @12 Err String, then free
    /// the wrapper) — NEVER the flat `DropListStr` that frees only the @12 handle and LEAKS the
    /// record's nested heap fields (HOLE-1). `None` for a `Result[String, String]` / a
    /// scalar-only record Ok (the flat path is sound there).
    pub(crate) fn result_ok_record_drop_fn(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Applied(TypeConstructorId::Result, a)
                if a.len() == 2 && matches!(a[1], Ty::String) =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }
    }

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
            // `ok(file_env + ["tail"])` — a LIST concat piece (porta resolve_env's
            // tail-duplicated arm). `try_lower_concat_list` yields a fresh owned list
            // (`__list_concat_rc` co-owns heap elements), movable into the Result
            // block exactly like the String-concat piece above.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                self.try_lower_concat_list(expr)
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

    /// `ok(<record>)` / `err(<String>)` for a `Result[heap-record, String]` (porta read_valtype's
    /// `ok({val, next})`). SAME cap-as-tag block as `materialize_result_str` (`is_err` selects the
    /// @16 tag; the payload — record handle for Ok, String for Err — is MOVED into @12), but the
    /// wrapper's drop must, at the Ok arm, RECURSIVELY free the record's nested heap fields. Route it
    /// to `$__drop_<drop_fn>` via `variant_drop_handles="resrec:<drop_fn>"` (→ [`Op::DropWrapperRec`]
    /// `is_result=true`, which tag-dispatches: Ok → `$__drop_<drop_fn>`, Err → flat `rc_dec` of the
    /// @12 String), NOT the flat `heap_elem_lists`/`DropListStr` (which would leak the Ok record's
    /// fields). Same cert as `materialize_result_str` (Alloc `i` + payload `m` + recursive `d`).
    pub(crate) fn materialize_result_aggregate(
        &mut self,
        piece: ValueId,
        repr: crate::Repr,
        is_err: bool,
        drop_fn: String,
    ) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::DynListStr { len: one } });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let off12 = self.const_add(oh, 12);
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![off12, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        let off16 = self.const_add(oh, 16);
        let tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tag, value: if is_err { 1 } else { 0 } });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![off16, tag] });
        self.variant_drop_handles.insert(obj, format!("resrec:{drop_fn}"));
        self.materialized_results_str.insert(obj);
        obj
    }

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
        if needs_rec {
            self.heap_elem_lists.remove(&obj);
            self.variant_drop_handles.insert(obj, format!("res_{type_name}"));
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
        // A heap-AGGREGATE element (tuple/record) is bound below as the slot's BORROWED block handle
        // (`LoadHandle` + registered in `materialized_aggregates`), so a direct FIELD/INDEX projection
        // (`for p in ps { p.0 }` / `for r in rs { r.x }`) projects off the ELEMENT block — the same
        // per-element borrow map/filter give a `List[record]`/`List[Value]` lambda param. A `let (x, y)
        // = p` destructure (tuple PATTERN) or passing `p` whole already worked; both now share the
        // element-precise borrow.
        let elem_is_aggregate = elem_heap && self.aggregate_field_tys(&elem_ty).is_some();
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
            // A heap-AGGREGATE element (tuple/record): register the borrowed block handle as a
            // materialized aggregate so a `p.0`/`r.x` field projection and a `let (x, y) = p`
            // destructure read the ELEMENT's slots (not the container) — the same per-element borrow
            // map/filter give an aggregate lambda param. The list still OWNS the element (its
            // recursive drop frees it), so this is a BORROW (already in `param_values`), not a second
            // owner — no double-free.
            if elem_is_aggregate {
                self.materialized_aggregates.insert(elem);
            }
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
