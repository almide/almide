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
        // A tail-position Result ctor the pointwise family above declined
        // (`ok((d, 2))` for `(Int, Int)!`, `ok((A{..}, B{..}))` for a
        // record-pair — the never-err `!`-lift value tail): the fn tail is a
        // DEGENERATE SINGLE ARM, so route it through the SAME
        // `lower_heap_result_arm` the heap-result `if`/`match` arms use.
        // That family is already the wide one — the identical payload passes
        // today the moment the body has any branch (an err arm, a two-ok
        // `if`) — the asymmetry was the tail spelling only. The arm helper
        // materializes the carrier and balances its own move-out, exactly as
        // in arm position; the value is moved out as the return.
        if matches!(tail.kind, IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. })
            && matches!(
                &tail.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                    if a.len() == 2
            )
        {
            let ty = tail.ty.clone();
            if let Some(dst) = self.lower_heap_result_arm(tail, &ty) {
                // The arm sub-paths end with `Op::Consume { dst }` — the
                // per-arm "moved into the merge" marker. In TAIL position
                // there is no merge: the move-out is the function return's
                // own `m`, and keeping the arm's marker double-moves the
                // carrier on the witness (`imm` — the kernel-proven checker
                // rejects it as an unowned dec; caught by the corpus-wall
                // PCC gate, Trust Spine red on 8c94f66c8). Remove that one
                // trailing marker — `Consume` is codegen-neutral, so the
                // rendered code is byte-identical; only the witness changes.
                // An arm path that never Consumed (the val-move-only style)
                // has nothing to remove and already balances via the ret `m`.
                if let Some(pos) = self
                    .ops
                    .iter()
                    .rposition(|op| matches!(op, Op::Consume { v } if *v == dst))
                {
                    self.ops.remove(pos);
                }
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
}
