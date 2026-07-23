impl LowerCtx {

    /// Extracted from `Self::lower_tail_heap_fresh_literals` (sixth-round split, cog
    /// reduction): the list/concat/interp construct sub-chain, verbatim.
    fn lower_tail_heap_fresh_list_concat_interp(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // A heap-ELEMENT list literal RETURNED — a `List[(String, String)]`
        // (`fn keyword_aliases() = [("Ok", "ok"), …]`) or a `List[Record]`
        // (`fn keyword_groups() = [KeywordGroup { … }, …]`, `fn precedence_table() =
        // [PrecLevel { … }, …]`). Build the real nested-ownership block (each element
        // moved in, the recursive per-element drop registered), MOVED OUT as the return
        // (NOT tracked → no scope-end drop; the caller owns it). Without this the literal
        // fell through `try_lower_str_list_literal` (which returns None for these heap
        // elements) to the Opaque alloc = an empty len-0 list (a silent miscompile).
        if let Some(dst) = self.try_lower_record_list_literal_tail(tail) {
            return Ok(Some(dst));
        }
        // A `List[String]` literal RETURNED (`fn make() = [e0, e1]`) — build a real
        // nested-ownership DynListStr (each element moved/Dup'd in), moved out as the
        // return (NOT tracked, so no scope-end DropListStr — the caller owns it). Without
        // this the literal fell to the Opaque alloc = an empty len-0 list.
        if let Some(dst) = self.try_lower_str_list_literal(tail) {
            return Ok(Some(dst));
        }
        // A scalar `List[Int/Float/Bool]` literal RETURNED with computed elements —
        // build + store each slot, moved out (an all-literal list is the Opaque/IntList
        // path below). Without this a `[a, a]` of computed scalars returned an empty list.
        if let Some(dst) = self.try_lower_scalar_list_construct(tail) {
            return Ok(Some(dst));
        }
        // A string concat RETURNED (`fn greet(n) = "Hi, " + n`) — a fresh owned String
        // (via __str_concat), moved out as the return (cert CallFn-result i + ret m).
        if let Some(dst) = self.try_lower_concat_str(tail) {
            return Ok(Some(dst));
        }
        // A SCALAR-element list concat RETURNED (`fn pair(xs) = xs + [7]`) — a fresh owned
        // list (via __list_concat), moved out as the return (cert CallFn-result i + ret m).
        // A heap-element list concat returns None and falls through to the deferred Opaque.
        if let Some(dst) = self.try_lower_concat_list(tail) {
            return Ok(Some(dst));
        }
        // A STRING INTERPOLATION RETURNED (`fn greet(n) = "Hi, ${n}"`) over the
        // executable subset — a fresh owned String (via the __str_concat chain),
        // moved out as the return. A compound/call-operand interp falls through to
        // the deferred Opaque below.
        if let IrExprKind::StringInterp { parts } = &tail.kind {
            if let Some(dst) = self.try_lower_string_interp(parts) {
                return Ok(Some(dst));
            }
        }
        Ok(None)
    }

    /// Extracted from `Self::lower_tail_heap_fresh` (fifth-round split, cog reduction):
    /// the Option/Result-ctor sub-chain + the Spread/Record-Consume retry + the final
    /// `Alloc{Opaque}`-or-wall fallback, verbatim.
    fn lower_tail_heap_fresh_ctors_and_opaque(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.lower_tail_heap_fresh_variant_ctors(tail)? {
            return Ok(Some(dst));
        }
        // A SPREAD-record (`{ ...n, gap_main: v }` — ceangal's with_* rebuilders) or a
        // plain RECORD literal RETURNED as the fn tail: the SAME construct machinery the
        // heap-result ARM position already uses (base slots copied via Dup — a borrowed
        // param base stays valid; overrides moved in), then MOVED OUT exactly per the arm
        // precedent (`Consume` + per-frame temp drops; the caller frees the return by its
        // type). A non-materialized base / out-of-subset field returns None → the honest
        // Opaque wall below.
        if matches!(&tail.kind, IrExprKind::SpreadRecord { .. }) {
            let mark = self.live_heap_handles.len();
            if let Some(dst) = self.try_lower_spread_record_construct(tail) {
                self.ops.push(Op::Consume { v: dst });
                self.drop_arm_locals(mark);
                return Ok(Some(dst));
            }
        }
        if matches!(&tail.kind, IrExprKind::Record { .. }) {
            let mark = self.live_heap_handles.len();
            if let Some(dst) = self
                .try_lower_record_construct(tail)
                .or_else(|| self.try_lower_scalar_record_construct(tail))
            {
                self.ops.push(Op::Consume { v: dst });
                self.drop_arm_locals(mark);
                return Ok(Some(dst));
            }
        }
        let repr = repr_of(&tail.ty)?;
        let init = alloc_init(tail);
        // `alloc_init` faithfully materializes a string literal and a scalar-
        // literal list/tuple (handled together with the faithful attempts above);
        // every other constructor (Map/Record/Result/Option/closure/range, a
        // non-foldable list) yields `Init::Opaque` — an EMPTY heap value the caller
        // would observe as the return = a SILENT MISCOMPILE. Reject the unfaithful
        // case explicitly.
        if matches!(init, Init::Opaque) {
            return Err(LowerError::Unsupported(format!(
                "heap-result {} cannot be faithfully returned in this brick \
                 (would move out an empty deferred heap value)",
                kind_name(&tail.kind)
            )));
        }
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc { dst, repr, init });
        self.record_elided_calls(tail);
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_heap_fresh` (fourth-round split, cog reduction):
    /// the Option/Result ctor + heap-`??` sub-chain, verbatim.
    fn lower_tail_heap_fresh_variant_ctors(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.lower_tail_heap_fresh_option_ctors(tail)? {
            return Ok(Some(dst));
        }
        self.lower_tail_heap_fresh_result_tuple_ctors(tail)
    }

    /// Extracted from `Self::lower_tail_heap_fresh_variant_ctors` (fifth-round split, cog
    /// reduction): the Option-ctor / heap-`??` / unit-effect-Ok sub-chain, verbatim.
    fn lower_tail_heap_fresh_option_ctors(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // A `Some(scalar)`/`None` RETURNED (`fn some_int(x) = Some(x)`) is
        // MATERIALIZED so the caller receives a real 0-or-1-element-list
        // Option (len-correct) it can `match` — the self-host Option fns
        // (list.get/first/last) return through such helpers. Moved out (NOT
        // pushed to live_heap_handles), cert = Alloc i + move-out m.
        // `ok(Pair(_e0, _e1))` / `ok(Plain)` / `err(msg)` for `Result[<user variant>, String]`
        // (derived variant decode) — materialize the variant Ok, recursive `$__drop_<V>` drop.
        // BEFORE the generic `try_lower_option_ctor` heap-Ok path, which would emit a dangling
        // `CallFn "Pair"` for the variant ctor.
        if let Some(dst) = self.try_lower_result_variant_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `err(Overflow(msg))` RETURNED for `Result[T_scalar, <user variant>]`
        // (the structured-error class): the len-as-tag Err wrapper, moved out.
        if self.is_scalar_ok_variant_err_result(&tail.ty) {
            if let Some(dst) = self.try_lower_result_err_variant_ctor(tail, &tail.ty) {
                self.live_heap_handles.retain(|h| *h != dst);
                return Ok(Some(dst));
            }
        }
        if let Some(dst) = self.try_lower_option_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `fn f() -> String = opt ?? "d"` — a heap-String `??` RETURNED. Executes via the
        // self-host `option.unwrap_or_str` call (try_lower_option_unwrap_or's heap branch),
        // MOVED OUT as the return (track_result=false — the caller owns it; tracking it
        // would double-free). Closes the tail-position heap-`??` silent-Opaque hole.
        if let IrExprKind::UnwrapOr { expr, fallback } = &tail.kind {
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, false) {
                return Ok(Some(dst));
            }
        }
        // `ok(<Unit expr>)` RETURNED (`ok(match parsed { ok(v) => println…, err(e)
        // => println… })` — the result_match_behind_ok_wrapper shape): the payload
        // is an EFFECT, not a value — run it through the statement dispatcher (the
        // unit match executes only the taken arm over its tracked subject), then
        // return the plain `ok(())` block. Effects are emitted BEFORE the ctor, so
        // a ctor decline after them must WALL (falling through would re-lower the
        // payload = double effects).
        if let IrExprKind::ResultOk { expr } = &tail.kind {
            if matches!(expr.ty, Ty::Unit)
                && matches!(
                    expr.kind,
                    IrExprKind::Match { .. }
                        | IrExprKind::If { .. }
                        | IrExprKind::Block { .. }
                        | IrExprKind::Call { .. }
                )
            {
                let payload = (**expr).clone();
                self.lower_stmt_expr(&payload)?;
                let unit_ok = IrExpr {
                    kind: IrExprKind::ResultOk {
                        expr: Box::new(IrExpr {
                            kind: IrExprKind::Unit,
                            ty: Ty::Unit,
                            span: None,
                            def_id: None,
                        }),
                    },
                    ty: tail.ty.clone(),
                    span: None,
                    def_id: None,
                };
                if let Some(dst) = self.try_lower_result_scalar_ok_ctor(&unit_ok, &tail.ty) {
                    return Ok(Some(dst));
                }
                return Err(LowerError::Unsupported(
                    "unit-payload `ok(<effect>)` return whose `ok(())` block is \
                     outside the ctor subset (the payload's effects were already \
                     emitted) not in this brick"
                        .into(),
                ));
            }
        }
        Ok(None)
    }

    /// Extracted from `Self::lower_tail_heap_fresh_variant_ctors` (fifth-round split, cog
    /// reduction): the Result-ctor family for record/option/value/tuple payloads,
    /// verbatim.
    fn lower_tail_heap_fresh_result_tuple_ctors(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // `ok({val, next})` / `err(msg)` RETURNED for a `Result[heap-record, String]` (porta
        // read_valtype): materialize the record-Ok / String-Err block, MOVED OUT as the
        // return (the recursive `Op::DropWrapperRec` frees it via `$__drop_<R>` at the
        // caller's scope end). Checked before the generic ctor paths below.
        if let Some(dst) = self.try_lower_result_record_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok(none)` / `ok(<Option[record]>)` / `err(msg)` RETURNED for `Result[Option[record],
        // String]` (porta read_message): recursive `$__drop_opt_<R>` via `resrec:opt_<R>`.
        if let Some(dst) = self.try_lower_result_option_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok(some(x))` / `ok(none)` / `err(msg)` RETURNED for `Result[Option[T], String]` with a
        // STRING / SCALAR leaf (the derived-Codec `__decode_option_T`): flat `DropListStr` for a
        // scalar Option, recursive `$__drop_opt_str` for a String Option. Checked AFTER the
        // record ctor (which claims the record-Option shape) — the leaf gate keeps them disjoint.
        if let Some(dst) = self.try_lower_result_option_scalar_str_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok(value.array(...))` / `err(msg)` RETURNED for a `Result[Value, String]` (csv
        // `parse`): materialize the Value-Ok / String-Err Result block, MOVED OUT as the
        // return (the recursive `Op::DropResultValue` frees it at the caller's scope end).
        if let Some(dst) = self.try_lower_result_value_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok((slice, pos))` / `err(msg)` RETURNED for a `Result[(String, Int), String]`
        // (toml `parse_key_part`): materialize the (String,Int)-Ok / String-Err block,
        // MOVED OUT (the recursive `Op::DropResultStrInt` frees it at the caller's scope end).
        if let Some(dst) = self.try_lower_result_str_int_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok((value.…, pos))` / `err(msg)` RETURNED for `Result[(Value, Int), String]`
        // (toml parse_val): materialize the (Value,Int)-Ok / String-Err block, MOVED OUT
        // (the recursive `Op::DropResultValueInt` frees it at the caller's scope end).
        if let Some(dst) = self.try_lower_result_value_int_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok((keys, pos))` / `err(msg)` RETURNED for `Result[(List[String], Int), String]`
        // (toml parse_key / parse_table_key): the recursive `Op::DropResultListStrInt`.
        if let Some(dst) = self.try_lower_result_list_str_int_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok((items, np))` / `err` for `Result[(List[Value], Int), String]` (collect_array_items).
        if let Some(dst) = self.try_lower_result_list_value_int_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        // `ok(())` / `ok(<scalar>)` RETURNED for a `Result[<non-heap>, String]` (porta
        // `run_foreground` / `ensure_porta_dir` `ok(())`): materialize the flat len-0 Ok
        // block, MOVED OUT as the return (its scope-end `DropListStr` frees just the block —
        // no nested heap). The heap-Ok cases (record/value/tuple/String) were intercepted
        // by the ctors above, so reaching here is exactly the scalar/Unit Ok the arm path
        // already lowers — only the TAIL position was missing it (this closed that gap).
        if let Some(dst) = self.try_lower_result_scalar_ok_ctor(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        Ok(None)
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// `Call{Named}` variant-ctor arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_call_named_ctor(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        match self.try_lower_variant_ctor(tail) {
            Some(dst) => Ok(Some(dst)),
            None => Err(LowerError::Unsupported(
                "variant constructor returned directly cannot be faithfully \
                 materialized in this brick (a heap/recursive field outside the \
                 executable subset)"
                    .into(),
            )),
        }
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// generic `Call{Named}` arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_call_named(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &tail.kind else { unreachable!() };
        let mark = self.live_heap_handles.len();
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        let repr = repr_of(&tail.ty)?;
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: name.as_str().to_string(),
            args: lowered,
            result: Some(repr),
        });
        // Free any OWNED-temp arg the call materialized (`f(string.replace(s,…), s)` — the
        // yaml `parse_number(string.replace(s,"_",""), s)` shape). A heap-result tail returns
        // `dst` directly (moved out, NOT in live_heap_handles), bypassing the function's
        // scope-end drops — so the materialized arg temp would LEAK (a parse loop OOMs).
        self.drop_arm_locals(mark);
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// `Call{Module}` arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_call_module(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &tail.kind else { unreachable!() };
        let mark = self.live_heap_handles.len();
        let dst = self.lower_pure_module_value_call(
            module.as_str(),
            func.as_str(),
            args,
            &tail.ty,
        )?;
        // Free any owned-temp arg materialized for the call — a heap-result tail moves out
        // `dst` and bypasses scope-end drops (see the Named case above), so the temp leaks.
        // `dst` is moved out (not in live_heap_handles) so it is never among the dropped.
        self.live_heap_handles.retain(|h| *h != dst);
        self.drop_arm_locals(mark);
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// Member/IndexAccess/MapAccess/TupleIndex heap-extraction arm body, verbatim (the
    /// arm never destructured `tail.kind` beyond the top-level match, so this helper
    /// doesn't either).
    fn lower_tail_heap_extraction(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let dst = self.lower_heap_extraction(tail)?;
        // A PRECISE field BORROW (`fn f(r) = r.name` over a materialized/param
        // record — the loaded slot handle is in `param_values`, the CONTAINER still
        // owns it) cannot be moved out as-is: the caller would drop it while the
        // container also drops it = a double-free. AUTO-ACQUIRE an OWNED reference
        // first (`Op::Dup` cert `a`, then move out cert `m` — exactly `let q = r.name;
        // q`), so the returned `am` is independent of the container's reference. A
        // container-grain `Dup` result (NOT a borrow — `lower_heap_extraction`'s
        // fallback already acquired its own reference) is moved out directly.
        if self.param_values.contains(&dst) {
            let owned = self.fresh_value();
            self.ops.push(Op::Dup { dst: owned, src: dst });
            return Ok(Some(owned)); // moved out, NOT tracked (no double-drop)
        }
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// If arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_if(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::If { cond, then, else_ } = &tail.kind else { unreachable!() };
        if let Some(dst) = self.try_lower_heap_result_if(cond, then, else_, &tail.ty) {
            return Ok(Some(dst));
        }
        // Outside the executable heap-result-if subset, the arms would linearize
        // and the RETURN value would be one deferred Opaque EMPTY heap object the
        // caller observes = a SILENT MISCOMPILE. Reject explicitly.
        Err(LowerError::Unsupported(
            "heap-result `if` outside the executable subset cannot be faithfully \
             returned in this brick (would move out an empty deferred heap value)"
                .into(),
        ))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// Match arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_match(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Match { subject, arms } = &tail.kind else { unreachable!() };
        // A single-arm tuple-destructure `match t { (offs, _) => offs }` extracting ONE
        // component — semantically `t.<i>` (the wasm-bindgen post-`fold` extraction).
        // Re-route through the proven `TupleIndex` tail extraction (a heap component is a
        // borrow auto-acquired into an owned move-out; a scalar one a value read).
        if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
            let synth = Self::synth_tuple_index(subject, idx, elem_ty);
            return self.lower_tail(Some(&synth));
        }
        // A CUSTOM variant (user ADT) subject with a HEAP result — tag@slot0 dispatch
        // with heap-result arms (ADT brick 4, e.g. recursive `to_string`).
        if let Some(dst) =
            self.try_lower_custom_variant_match(subject, arms, &tail.ty)
        {
            return Ok(Some(dst));
        }
        // A heap-result VARIANT (Option/Result) match (`match scan_quote(..) {
        // some(p) => "..", none => ".." }`) over a SCALAR payload — the
        // subject-drop-before-arms desugar (cert-clean, scalar-payload only; a heap
        // payload self-gates back to None here = the true Camp-4 frontier).
        if is_variant_ty(&subject.ty) {
            if let Some(dst) =
                self.try_lower_variant_value_match(subject, arms, &tail.ty)
            {
                return Ok(Some(dst));
            }
        }
        // A len-as-tag RESULT subject with HEAP-result arms — the merge-based
        // value match (the Camp-4 `compute` opener; borrowed payload binds, the
        // subject temp freed by the scope epilogue after the merge move-out).
        if let Some(dst) = self.try_lower_result_match_value(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // An `Option[<heap>]` subject with HEAP-result arms — the Option twin
        // (is_balanced's fold step: `match acc { none => none, some(stack) => … }`).
        if let Some(dst) = self.try_lower_option_match_value(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // A LIST subject (`match xs { [] => .., ys => .. }`) with HEAP-result
        // arms — the len-tag twin of the Result opener (a bind-all arm aliases
        // the owned subject temp; release parity covers an arm move-out).
        if let Some(dst) = self.try_lower_list_match_value(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // A TUPLE subject of SCALAR elements with HEAP-result arms (`match (n % 3,
        // n % 5) { (0, 0) => "FizzBuzz", … }` — the fizz shape, the CHEATSHEET's
        // canonical match idiom): the ordered tuple-refinement chain, extended to
        // heap merges (per-arm `lower_heap_result_arm` + release parity).
        if let Some(dst) = self.try_lower_tuple_refinement_match(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // `desugar_match_to_if` wraps its OUTPUT in a `Block` (hoisted `let`s
        // preceding the `If`) whenever the subject isn't one of `subject_pure`'s
        // freely-substitutable kinds (`Var`/`LitInt`/`LitBool`/`LitFloat` —
        // notably missing `LitStr`: a single-use `let s = "hello world"` subject
        // gets constant-propagated to a bare `LitStr` upstream, same gap B52
        // fixed for the call-argument consumer in `calls_p2.rs`). Unwrap it
        // generically here too — lower the hoisted `let`s via `self.lower_stmt`,
        // then extract the inner `If` — rather than widening `subject_pure`
        // itself (a general fix, not LitStr-specific: ANY subject needing the
        // hoist now works in this tail position too).
        let lifted = self.desugar_match_to_if(subject, arms, &tail.ty).and_then(|e| {
            let (stmts, if_expr) = match e.kind {
                IrExprKind::If { .. } => (Vec::new(), e),
                IrExprKind::Block { stmts, expr: Some(t) } => (stmts, *t),
                _ => return None,
            };
            let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return None };
            for s in &stmts {
                self.lower_stmt(s).ok()?;
            }
            self.try_lower_heap_result_if(cond, then, else_, &tail.ty)
        });
        if let Some(dst) = lifted {
            return Ok(Some(dst));
        }
        // Outside the executable heap-result-match subset, the RETURN value would
        // be one deferred Opaque EMPTY heap object the caller observes = a SILENT
        // MISCOMPILE. Reject explicitly.
        Err(LowerError::Unsupported(
            "heap-result `match` outside the executable subset cannot be faithfully \
             returned in this brick (would move out an empty deferred heap value)"
                .into(),
        ))
    }

    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// `Call{Computed}` closure arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_call_computed(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &tail.kind else { unreachable!() };
        let mark = self.live_heap_handles.len();
        let blk = self.closure_value_of(callee).expect("the caller's match guard already proved closure_value_of(callee).is_some() for the same callee");
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        let repr = repr_of(&tail.ty)?;
        self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
        self.drop_arm_locals(mark);
        Ok(Some(dst))
    }

    /// The SCALAR tail of [`Self::lower_tail`] (Copy value, no ownership).
    /// Verbatim text move.
    fn lower_tail_scalar(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // Scalar return value (Copy — no ownership accounting). A scalar `BinOp`/
        // `UnOp` is a FRESH computed scalar (arithmetic / comparison / logic), so it
        // is a `Const` like a literal — its operands carry their own ownership.
        match &tail.kind {
            IrExprKind::Var { id } => Ok(Some(self.value_or_global(*id)?)),
            // A scalar-result resolvable CALL tail (`fn f() = g()`, `= add(2, 3)`,
            // `= string.len(s)`): a real executable `CallFn` (args materialized, the
            // scalar result returned). An unresolvable Method/Computed callee (or an
            // unsupported arg) falls through to the deferred `Const` + elided-caps
            // marker below — the call is captured for caps, its value deferred.
            IrExprKind::Call { .. } => {
                if let Some(dst) = self.try_lower_scalar_call(tail, &tail.ty) {
                    return Ok(Some(dst));
                }
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // An INT literal materializes to a real value (the scalar-value
            // foundation): `ConstInt` renders `(i64.const v)`, so a fn returning a
            // literal returns the right value, not the deferred-`Const` zero. This is
            // what lets a self-hosted runtime fn compute real offsets/lengths.
            IrExprKind::LitInt { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value });
                Ok(Some(dst))
            }
            // A FLOAT literal returned directly (`fn pi() = 3.14159`) materializes its REAL f64
            // BITS as a `ConstInt` (the i64-uniform Float repr), so the fn returns the constant,
            // not the deferred-`Const` zero — the same materialization `lower_scalar_value` does
            // for a LitFloat operand. (The frontend folds `{ let p = 3.14; p }` to this form.)
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: crate::lower::float_lit_bits(*value, &tail.ty) });
                Ok(Some(dst))
            }
            // A BOOL literal returned directly (`(x) => true` — a constant/param-ignoring predicate
            // for list.all/any/count, or `fn t() = true`) materializes its 0/1 as a `ConstInt`, NOT
            // the deferred-`Const` ZERO it used to fall into below (which made `(x) => true` return
            // FALSE — a silent miscompile of every constant-true predicate). Bool is an i64 0/1, the
            // same materialization lower_scalar_value does for a LitBool operand.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value as i64 });
                Ok(Some(dst))
            }
            // A scalar Int Add/Sub/Mul computes its REAL value (IntBinOp over
            // recursively-lowered operands), so a fn `add(a, b) = a + b` returns the
            // sum — not the deferred-Const zero. Outside the int-arith subset (Div/
            // Mod/cmp/logic/Float) it rolls back and falls through to the Const below.
            // A scalar Int Add/Sub/Mul OR a scalar prim-floor call (`= prim.load32(a)`)
            // computes a real value via lower_scalar_value (IntBinOp / Op::Prim);
            // outside the subset it rolls back to the deferred Const + elided marker.
            IrExprKind::BinOp { .. } | IrExprKind::RuntimeCall { .. } => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A SCALAR field / tuple element / list element TAIL (`(p) => p.x`, `fn fst(t) = t.0`,
            // `fn at(xs, i) = xs[i]`) — LOAD the real value from the materialized aggregate's layout
            // slot (the VALUE MODEL read side, what makes `list.map(points, (p)=>p.x)` return the
            // real field); `xs[i]` is the bounds-checked `$elem_addr` load. `lower_scalar_value`
            // dispatches each. Outside the materialized subset it rolls back to the deferred `Const`
            // (its container's calls elided), exactly as before.
            IrExprKind::Member { .. }
            | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A scalar UNARY op RETURNED directly (`fn ineg(n) = -n`, `fn flip(b) = not b`,
            // `fn fneg(x) = -x`) computes its REAL value via `lower_scalar_value` (the
            // UnOp arm: int neg `0 - x`, float neg the `f64.neg` prim, bool `not` `1 - b`)
            // — NOT the deferred-`Const` zero this used to fall into. This is the TAIL-
            // position twin of the value-position UnOp fix: a function whose body IS a
            // `UnOp` is a value position, so it must compute, not read 0. Outside the
            // scalar subset (a non-lowerable operand) it rolls back to the Const below,
            // exactly like the `BinOp` tail arm.
            IrExprKind::UnOp { .. } if !is_heap_ty(&tail.ty) => {
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(tail) {
                    return Ok(Some(dst));
                }
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A SCALAR map extraction is an unambiguous COPY (a scalar is never
            // reference-counted), so it is a `Const` — its container carries its own
            // ownership. (A HEAP extraction is an ALIAS / share — it needs a layout-aware
            // field-access op with `Dup` semantics and stays walled until that brick.)
            IrExprKind::MapAccess { .. }
            // A SCALAR error-operator result (`x?.f` yielding a scalar) is
            // likewise a fresh `Const`; the operator's value + early-return are deferred.
            | IrExprKind::Try { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE returned: a fresh `Const` (no ownership); any analyzable callee
            // inside it is captured for caps by `record_elided_calls`. (A scalar-result
            // CALL is handled by its own arm above — a real executable `CallFn` when
            // resolvable, else the same deferred `Const` + elided marker.)
            | IrExprKind::Range { .. } => {
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A TAIL `e!` (Unwrap — effect-fn error propagation): `f() = g()!` propagates g's
            // Result unchanged, i.e. it IS `f() = g()`. Strip the `!` and lower `e` as the tail.
            IrExprKind::Unwrap { expr } => self.lower_tail(Some(expr)),
            // A SCALAR tail `??` (`fn parse_or_zero(s) = int.parse(s) ?? 0`, the canonical
            // form) EXECUTES the unwrap (tag read + payload-or-fallback) — it was a deferred
            // `Const` 0 here (a silent wrong value, neither payload nor fallback). Outside the
            // executable subset a `??` over a VARIANT operand WALLs (a Const-0 would be wrong);
            // a non-variant operand keeps the deferred `Const`.
            IrExprKind::UnwrapOr { expr, fallback } => {
                if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, false) {
                    return Ok(Some(dst));
                }
                if is_variant_ty(&expr.ty) {
                    return Err(LowerError::Unsupported(
                        "?? over an Option/Result operand in tail position outside the \
                         executable subset cannot be faithfully computed (a Const-0 would be \
                         a wrong value) not in this brick"
                            .into(),
                    ));
                }
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(tail);
                Ok(Some(dst))
            }
            // A scalar `if` tail EXECUTES (only the taken arm runs) via try_lower_scalar_if
            // — the IfThen/Else/EndIf markers — when the cond + both arms are in the
            // scalar subset; otherwise it falls back to the deferred linearize + Const.
            IrExprKind::If { cond, then, else_ } => {
                if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, &tail.ty) {
                    return Ok(Some(dst));
                }
                self.lower_branch(tail)?;
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                Ok(Some(dst))
            }
            // A scalar-result `match` over INT literal patterns EXECUTES: desugar to a
            // nested `if subject == lit then arm else …` and lower it via the scalar-if
            // machinery (only the matched arm runs). Non-literal patterns / guards / a
            // non-scalar subject fall back to the deferred linearize + merged `Const`.
            IrExprKind::Match { subject, arms } => {
                // A single-arm tuple-destructure `match t { (_, n) => n }` extracting ONE SCALAR
                // component — semantically `t.<i>` (the tuple-accumulator `fold` cursor extraction).
                // Lower the synthetic `TupleIndex` via the scalar value model (a real slot load).
                if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
                    if !is_heap_ty(&elem_ty) {
                        let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                        let mark = self.ops.len();
                        if let Some(dst) = self.lower_scalar_value(&synth) {
                            return Ok(Some(dst));
                        }
                        self.ops.truncate(mark);
                    }
                }
                // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
                // `fn val(t: Tok) -> Int = match t { Num(n) => n, … }`.
                if let Some(dst) =
                    self.try_lower_custom_variant_match(subject, arms, &tail.ty)
                {
                    return Ok(Some(dst));
                }
                // A TUPLE subject of scalar elements/expressions with a SCALAR result
                // (`match (a % 2, b % 3) { (0, 0) => 100, … }`) — the ordered
                // refinement chain (the scalar sibling of the heap-tail hook).
                if let Some(dst) = self.try_lower_tuple_refinement_match(subject, arms, &tail.ty) {
                    return Ok(Some(dst));
                }
                // A VARIANT (Option/Result) subject returned by a function — execute the
                // tag-read value-match (only the taken arm runs, the scalar payload bound);
                // `fn pick(o) = match o { Some(x) => x, None => -1 }` is the canonical form.
                // A ctor pattern is not `subj == lit`, so it can't reach `desugar_match_to_if`.
                if is_variant_ty(&subject.ty) {
                    if let Some(dst) = self.try_lower_variant_value_match(subject, arms, &tail.ty) {
                        return Ok(Some(dst));
                    }
                    // A UNIT-result tail variant match (`match write_summary(..) { ok(p) =>
                    // {…effects…}, err(e) => {…effects…} }` — the run_all_finish shape): the arms
                    // produce no VALUE, only effects, so there is nothing to "pick" — this is
                    // exactly the statement/Unit-position dispatch `lower_branch` already executes
                    // (track the Result subject → `try_lower_result_match` reads the tag and runs
                    // ONLY the taken arm; an untrackable subject linearizes both arms, the
                    // existing caps-union-sound fallback). DELEGATE to it rather than wall — the
                    // function's Unit return is the merged `Const` below (no value escapes the
                    // branch). The same proven machinery every non-tail Unit match uses; gated to
                    // `Unit` so a SCALAR/HEAP-result variant match (whose value DOES matter) keeps
                    // walling here (`lower_branch` would discard its value = a silent miscompile).
                    if matches!(tail.ty, Ty::Unit) {
                        self.lower_branch(tail)?;
                        let dst = self.fresh_value();
                        if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                        return Ok(Some(dst));
                    }
                    return Err(LowerError::Unsupported(
                        "variant (Option/Result) match in tail position outside the \
                         executable subset cannot be faithfully computed (a Const-0 would \
                         silently pick a wrong arm) not in this brick"
                            .into(),
                    ));
                }
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &tail.ty) {
                    // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
                    // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                        return Ok(Some(dst));
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
                self.lower_branch(tail)?;
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("tail"));
                }
                self.ops.push(Op::Const { dst });
                Ok(Some(dst))
            }
            other => Err(LowerError::Unsupported(format!(
                "scalar tail {} not in this brick",
                kind_name(other)
            ))),
        }
    }
}
