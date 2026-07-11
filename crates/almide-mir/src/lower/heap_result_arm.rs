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
                if crate::lower::is_variant_ty(&subject.ty)
                    || self.custom_variant_type_name(&subject.ty).is_some() =>
            {
                let arm_mark = self.live_heap_handles.len();
                // Option/Result subjects via the value match; a CUSTOM-variant subject (the
                // regrouped `err($q)` INNER match over a borrowed variant payload — the
                // `compute` class) via the tag@slot0 dispatcher, which accepts a heap result
                // over a BORROWED subject (the recursive-to_string precedent).
                let obj = match self.try_lower_variant_value_match(subject, arms, result_ty) {
                    Some(v) => v,
                    _ => self.try_lower_custom_variant_match(subject, arms, result_ty)?,
                };
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
            // A heap-result call through a KNOWN funcref arm (`Leaf(v) => leaf(v)`,
            // `Node(l, r) => merge(…)` — tree_fold's arms call fn-typed PARAMS): execute
            // via the closure-table call, the tail-position machinery ported per-arm
            // (tail.rs's Computed case). Same per-arm `"im"` balance as the Named-call
            // arm: the indirect call returns a FRESH owned heap value (`i`), the arm's
            // `Consume` moves it out (`m`); arg temps free within the arm. An UNKNOWN
            // callee falls through to the C1 direct-lambda inline case below.
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if is_heap_ty(&arm.ty) && self.closure_value_of(callee).is_some() =>
            {
                let arm_mark = self.live_heap_handles.len();
                let blk = self.closure_value_of(callee)?;
                let lowered = self.lower_call_args(args).ok()?;
                let obj = self.fresh_value();
                let repr = repr_of(result_ty).ok()?;
                self.emit_closure_call(blk, Some(obj), lowered, Some(repr));
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
            // `some((i, s))` — an `(Int, String)` TUPLE payload (the zip_first merge arm:
            // `(some(a), some(b)) => some((a, b))` after the tuple-variant desugar). The fresh
            // owned tuple (`lower_owned_heap_field` — literal construct or borrowed-Var Dup)
            // moves into the 1-element Option whose scope drop is the RECURSIVE
            // `$__drop_list_int_str` (`materialize_opt_int_str_some`, which Consumes the piece)
            // — the same shape as try_lower_option_ctor's `list.find` case. Per-arm `"im"`
            // balance: the Option `Alloc` (`i`) + the move-out `Consume` (`m`).
            IrExprKind::OptionSome { expr }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2
                        && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                let arm_mark = self.live_heap_handles.len();
                let repr = repr_of(result_ty).ok()?;
                let piece = self.lower_owned_heap_field(expr)?;
                let obj = self.materialize_opt_int_str_some(piece, repr);
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
}
