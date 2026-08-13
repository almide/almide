impl LowerCtx {
    /// Lower ONE arm of a heap-result `if` to the value the arm leaves on the wasm stack.
    /// A string LITERAL is `Alloc{Str}` + `Consume` (the per-arm `"im"` move-out balance —
    /// NOT added to `live_heap_handles`, it is moved out as the result). A NESTED `if` (a
    /// desugared `match`'s else-if) recurses, its result dst being this arm's value.
    ///
    /// Every caller emits this arm under REAL `IfThen`/`Else`/`EndIf` markers — exactly
    /// one arm runs — so the arm raises `in_frame` AND `unit_arm_depth` for its whole
    /// extent, the discipline `lower_scalar_arm` carries (C-188/#907). It raised NEITHER,
    /// so context-sensitive machinery read the arm as straight-line code: `memo_global`
    /// cached a heap global materialized in the THEN arm and the ELSE arm then compared
    /// against a value whose defining alloc sits in the untaken block — `s == TAG` with
    /// `TAG` mentioned in both arms answered `else-miss` for the very string TAG holds
    /// (#945's heap twin; the scalar `CAP` edition was the reported glTF-loader bug).
    fn lower_heap_result_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        self.in_frame += 1;
        self.unit_arm_depth += 1;
        let r = self.lower_heap_result_arm_unmarked(arm, result_ty);
        self.unit_arm_depth -= 1;
        self.in_frame -= 1;
        r
    }

    /// [`Self::lower_heap_result_arm`]'s body, verbatim — split so the counter raise
    /// brackets every early return.
    fn lower_heap_result_arm_unmarked(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        // A HEAP-Ok PAYLOAD arm (`if age < 200 then "valid" else err(x)!` in a
        // `Result[String, _]` fn — the guard-chain tail): the arm value is the Ok
        // PAYLOAD, not the Result — returning it bare puts a raw String where the
        // caller reads a cap-as-tag wrapper (the validate_age latent miscompile).
        // Build the Ok wrapper (`lower_result_str_piece` + `materialize_result_str`),
        // the heap twin of the scalar-Var `materialize_result_ok` arm below. Gated to
        // PAYLOAD-shaped arms whose ty IS the Ok payload — LitStr/Var/concat, and
        // (#841) CALL arms: a CAN-ERR lifted effect fn now always-wraps (its body ty
        // is overridden to `Result[T, String]`), so its ok arms are payload-typed
        // calls (`value.object(..)`, `value.int(n)`) that must wrap here — left bare
        // they returned raw `T` against the wrapped-Err propagation paths, the
        // undiscriminable hybrid ABI. A Result-typed arm (Unwrap/ctor/nested
        // if/block, or a call whose OWN type is the Result) keeps its own path.
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) =
            result_ty
        {
            if a.len() == 2
                && a[0] == arm.ty
                && is_heap_ty(&a[0])
                && matches!(
                    &arm.kind,
                    IrExprKind::LitStr { .. }
                        | IrExprKind::Var { .. }
                        | IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. }
                        | IrExprKind::Call { .. }
                )
            {
                let arm_mark = self.live_heap_handles.len();
                let piece = self.lower_result_str_piece(arm)?;
                let repr = repr_of(result_ty).ok()?;
                // A `Result[Value, _]` wrapper drops via the recursive
                // DropResultValue (value_ok), a String one via the flat DropListStr.
                let obj = self.materialize_result_str(
                    piece,
                    repr,
                    false,
                    crate::lower::is_value_ty(&a[0]),
                );
                self.ops.push(Op::Consume { v: obj });
                self.drop_arm_locals(arm_mark);
                return Some(obj);
            }
        }
        let out = self
            .lower_heap_result_arm_literal(arm, result_ty)
            .or_else(|| self.lower_heap_result_arm_option(arm, result_ty))
            .or_else(|| self.lower_heap_result_arm_result(arm, result_ty))
            .or_else(|| self.lower_heap_result_arm_ctrl(arm, result_ty));
        if out.is_none() {
            crate::trace::trace("ALMIDE_DBG_ELEM", || {
                format!(
                    "[heap-if-arm] declined arm kind {} (result ty {:?})",
                    crate::lower::kind_name(&arm.kind),
                    result_ty
                )
            });
        }
        out
    }

    /// Router for [`Self::lower_heap_result_arm`]'s four arm-kind groups (split out for
    /// codopsy cognitive-complexity — pure text-move, no behavior change): `_literal` (control/
    /// concat/aggregate-literal arms), `_option` (all `OptionSome`/`OptionNone` guards),
    /// `_result` (all `ResultOk`/`ResultErr` guards), `_ctrl` (Match/Block/Call/field-projection
    /// arms + the trailing catch-all). Groups are non-overlapping by `IrExprKind` discriminant
    /// EXCEPT `Match{..}` and `Call{target: Computed{..}, ..}`, which each appear TWICE (a
    /// guarded fast path + a generic fallback) and are co-located inside `_ctrl` in their
    /// original relative order — Rust's match "first true guard commits" semantics requires
    /// both arms of such a pair to live in the SAME `match` statement, so they were kept
    /// together rather than split by textual position.
    /// Control/concat/aggregate-literal arms: `Unwrap`/`Try` pass-through, `UnwrapOr`,
    /// `LitStr`, `Var` (Result-Ok-context + generic), string/list concat, `StringInterp`,
    /// nested `If`, and the `List`/`Tuple`/`Record`/`SpreadRecord` literal constructors.
    /// Verbatim subset of the original single match — each arm owns its own `arm_mark`/
    /// `drop_arm_locals` frame, no state crosses arm boundaries. Split further (codopsy
    /// cc) into `_a` (pass-through + the two `Var` arms, which MUST stay together — the
    /// guarded arm's failed guard falls through to the plain `Var` arm in the SAME match,
    /// exactly as Rust's guard semantics require) and `_b` (concat/interp/nested-if/
    /// aggregate-literal arms, a disjoint `IrExprKind` discriminant set from `_a`'s).
    fn lower_heap_result_arm_literal(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        self.lower_heap_result_arm_literal_a(arm, result_ty)
            .or_else(|| self.lower_heap_result_arm_literal_b(arm, result_ty))
    }

    fn lower_heap_result_arm_literal_a(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            // An `e!` arm (`if c then parse_sequence(..)! else ..`) — effect-fn error
            // propagation: `e!` returns e's Result unchanged (Ok→Ok, Err→Err), so strip the
            // `!` and lower `e` as the arm (the same identity the tail-position `e!` uses).
            // `Try` is the frontend auto-`?` — in a Result-typed arm both it and a spelled
            // `!` propagate the inner call's same-repr Result verbatim (the pass-through).
            // …but ONLY in a RESULT-typed arm. When the arm must yield the PAYLOAD (`result_ty`
            // IS the arm's own type while `e` is the wrapping Result — the `??`-with-inline-`!`
            // fallback, #1375) `e! ≡ e` is NOT the identity: passing `e` through leaves the whole
            // Result BLOCK where the merge expects the payload handle, and the reader picks the
            // ok-discriminant out of it (`json.parse("hi") ?? json.parse("[1,2]")!` printed
            // `true`). DECLINE — a silent wrong value is never an acceptable arm value; the
            // caller rolls back to its own wall, and `desugar_unwrap_or_unwrap_fallback` has
            // already rewritten the `??` spelling to the `(match … )!` form that DOES execute.
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }
                if *result_ty == arm.ty && expr.ty != arm.ty =>
            {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!(
                        "[heap-if-arm] declined payload-typed `e!` arm (arm ty {:?}, inner ty {:?})",
                        arm.ty, expr.ty
                    )
                });
                None
            }
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                self.lower_heap_result_arm(expr, result_ty)
            }
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
            IrExprKind::Var { id }
                if !is_heap_ty(&arm.ty)
                    && matches!(result_ty,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                            if a.len() == 2 && a[0] == arm.ty) =>
            {
                let payload = self.value_for(*id).ok()?;
                let repr = repr_of(result_ty).ok()?;
                let obj = self.materialize_result_ok(payload, repr);
                self.ops.push(Op::Consume { v: obj });
                Some(obj)
            }
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.ops.push(Op::Consume { v: dst });
                Some(dst)
            }
            _ => None,
        }
    }

    /// Concat/interp/nested-if/aggregate-literal arms — the rest of the original single
    /// match, a discriminant set disjoint from `_a`'s (no `IrExprKind` variant appears in
    /// both halves), so splitting the match at this boundary changes nothing observable.
    /// Further split into `_b` (concat/interp/nested-if) and `_c` (the four aggregate
    /// literal constructors) — same disjoint-discriminant reasoning.
    fn lower_heap_result_arm_literal_b(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        self.lower_heap_result_arm_literal_b1(arm, result_ty)
            .or_else(|| self.lower_heap_result_arm_literal_b2(arm, result_ty))
    }

    fn lower_heap_result_arm_literal_b1(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
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
            _ => None,
        }
    }

    fn lower_heap_result_arm_literal_b2(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
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
            _ => None,
        }
    }

    /// Every `OptionSome` payload-shape guard (record / variant-ctor payload / tuple
    /// combos / generic heap / generic scalar) plus `OptionNone`. Guard order is
    /// load-bearing (most-specific payload shape first) — kept verbatim from the original
    /// single match, unchanged relative order.
    fn lower_heap_result_arm_option(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::OptionSome { expr } => self.lower_heap_result_arm_some(expr, result_ty),
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
            _ => None,
        }
    }

    /// The `some(<payload>)` half of [`Self::lower_heap_result_arm_option`]'s
    /// router, by payload shape: the AGGREGATE payloads first (each with its own
    /// exact drop routing — [`Self::arm_some_aggregate`]), then a general heap
    /// payload, then a scalar one.
    ///
    /// `arm_some_aggregate`'s outer `Option` is the SELECTION and its inner one the
    /// RESULT, so an aggregate arm whose body declines still ends the whole
    /// lowering — never falls through to the heap path. That commit-once behavior
    /// is what the former single `match`'s `?`-in-arm-body spelled, and several arm
    /// bodies genuinely rely on it.
    fn lower_heap_result_arm_some(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        if let Some(selected) = self.arm_some_aggregate(expr, result_ty) {
            return selected;
        }
        if is_heap_ty(&expr.ty) {
            let repr = repr_of(result_ty).ok()?;
            // The owned String payload: a let-bound Var (its handle), or a direct user-call
            // that RETURNS a fresh owned String (CallFn result, rc 1) — materialized into the
            // Option below (its `Consume` `m` balances the alloc/call `i`).
            let piece = self.arm_some_heap_piece(expr)?;
            let obj = self.materialize_opt_str_some(piece, repr);
            self.ops.push(Op::Consume { v: obj });
            return Some(obj);
        }
        let payload = self.lower_scalar_value(expr)?;
        let repr = repr_of(result_ty).ok()?;
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc { dst: obj, repr, init: Init::OptSome { payload } });
        self.ops.push(Op::Consume { v: obj });
        Some(obj)
    }

    /// The AGGREGATE `some(<payload>)` arms — a heap record, a custom-variant ctor,
    /// or one of the four tuple shapes — each of which needs its OWN drop routing
    /// rather than the generic heap path's flat `DropListStr`. `None` means "no arm
    /// selected"; `Some(None)` means an arm selected and declined (see
    /// [`Self::lower_heap_result_arm_some`]). Arms are in their original order.
    fn arm_some_aggregate(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<Option<ValueId>> {
        Some(match &expr.kind {
            IrExprKind::Record { .. }
                if self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                self.arm_some_record(expr, result_ty)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                self.arm_some_variant_ctor(expr, result_ty)
            }
            IrExprKind::Tuple { .. }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2
                        && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::String)) =>
            {
                self.arm_some_str_str_tuple(expr, result_ty)
            }
            _ if matches!(&expr.ty,
                Ty::Tuple(tys) if tys.len() == 2
                    && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                self.arm_some_int_str_tuple(expr, result_ty)
            }
            IrExprKind::Tuple { .. }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                self.arm_some_scalar_tuple(expr, result_ty)
            }
            IrExprKind::Tuple { .. }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
                        && !is_heap_ty(&tys[1])) =>
            {
                self.arm_some_str_scalar_tuple(expr, result_ty)
            }
            _ => return None,
        })
    }

    /// A `some(<record>)` arm — Option wrapping a heap RECORD (porta find_eq_pos's
    /// `some({key: key, val: val})`). Materialize the owned record payload
    /// (`try_lower_record_construct`, recursive-drop), wrap it in the 0-or-1 Option, and route
    /// the Option's scope-end drop to the recursive `$__drop_<R>` (`Op::DropWrapperRec`) so the
    /// record's nested heap fields are freed — NOT the flat `DropListStr` that leaks them. Same
    /// per-arm `"im"` balance (Alloc `i` + the move-out `Consume` `m`); the record-construct's
    /// transient temps are freed within the arm (`drop_arm_locals`). Gated on the record needing
    /// a recursive drop (`record_or_anon_drop_type_name`) — a scalar-only record has no
    /// `$__drop_<R>` and is not reached here (it would fall through to the deferred path).
    fn arm_some_record(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let drop_fn = self.record_or_anon_drop_type_name(&expr.ty)?;
        let piece = self.try_lower_record_construct(expr)?;
        let obj = self.materialize_opt_aggregate_some(piece, repr, drop_fn);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// `some(Number(7))` — Some wrapping a CUSTOM-VARIANT ctor payload as a MATCH/if ARM
    /// value (the option-of-variant shape, `try_lower_option_ctor`'s BIND-position twin,
    /// binds_p4.rs — never mirrored here). Build the variant block
    /// (`try_lower_variant_ctor`), move it into the 1-element Option. Drop routing by the
    /// payload's OWN discipline: a recursive-drop variant routes "optrec:<Type>" → the
    /// generated `$__drop_<Type>` frees the payload (fields, then block) then the option
    /// block; a flat variant (no heap fields) uses the Some(string) shape — DropListStr's
    /// flat slot-0 free IS its exact drop. Checked BEFORE the generic Named-call arm
    /// further below (a ctor is NOT a real wasm fn — `try_lower_variant_ctor` inlines its
    /// block construction at every call site, so the plain Named-call route would emit an
    /// unlinked call).
    fn arm_some_variant_ctor(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind
        else {
            return None;
        };
        let type_name = self.variant_layouts.ctor_to_type.get(name.as_str())?.clone();
        let needs_rec = self.variant_layouts.needs_recursive_drop(&type_name, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        });
        let piece = self.try_lower_variant_ctor(expr)?;
        let obj = if needs_rec {
            self.materialize_opt_aggregate_some(piece, repr, type_name)
        } else {
            self.materialize_opt_str_some(piece, repr)
        };
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// `some((s1, s2))` — a `(String, String)` TUPLE payload as an if/match ARM
    /// value (the if-merged Option ctor, `try_lower_option_ctor`'s bind-position
    /// twin — the fuzz index-374 divergence): build the tuple (both slots owned
    /// Strings), move it into the 1-element Option routed to
    /// `$__drop_opt_str_str`. Per-arm `"im"` balance (Alloc `i` + move-out
    /// `Consume` `m`); transient temps freed within the arm.
    fn arm_some_str_str_tuple(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else { return None };
        let elements = elements.clone();
        let piece = self.try_lower_tuple_construct(&elements)?;
        let obj = self.materialize_opt_str_some(piece, repr);
        self.variant_drop_handles.insert(obj, "opt_str_str".to_string());
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// `some((i, s))` — an `(Int, String)` TUPLE payload (the zip_first merge arm:
    /// `(some(a), some(b)) => some((a, b))` after the tuple-variant desugar). The fresh
    /// owned tuple (`lower_owned_heap_field` — literal construct or borrowed-Var Dup)
    /// moves into the 1-element Option whose scope drop is the RECURSIVE
    /// `$__drop_list_int_str` (`materialize_opt_int_str_some`, which Consumes the piece)
    /// — the same shape as try_lower_option_ctor's `list.find` case. Per-arm `"im"`
    /// balance: the Option `Alloc` (`i`) + the move-out `Consume` (`m`).
    fn arm_some_int_str_tuple(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let piece = self.lower_owned_heap_field(expr)?;
        let obj = self.materialize_opt_int_str_some(piece, repr);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// `some((x, y))` — an ALL-SCALAR tuple literal payload as a MATCH/if ARM value
    /// (`match e { Click{x,y,..} => some((x,y)), _ => none }` — extract_click_positions,
    /// the closure body a `list.filter_map` lambda lifts). Build the flat tuple block,
    /// move it into the 1-element Option: the payload owns NO inner heap, so
    /// `materialize_opt_str_some`'s flat slot-0 free is EXACT (the SAME shape
    /// `try_lower_option_ctor`'s BIND-position twin already proves, binds_p4.rs — this
    /// arm-position mirror was simply never added). Checked BEFORE the generic
    /// `is_heap_ty` fallback below, whose inner `match &expr.kind` has no `Tuple` case
    /// (it only covers Var / Named-call / pure-String-Module-call payloads) and would
    /// otherwise decline a Tuple literal outright.
    fn arm_some_scalar_tuple(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else { return None };
        let elements = elements.clone();
        let piece = self.try_lower_scalar_tuple_construct(&elements)?;
        let obj = self.materialize_opt_str_some(piece, repr);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// `some((k, v))` — a `(String, <scalar>)` tuple literal payload as a MATCH/if ARM
    /// value (`map.find`'s `__skv_find_some(k, v) = Some((kc, v))`, B41's find-with-
    /// fallback shape). Build the tuple (`try_lower_tuple_construct`, one heap slot —
    /// the String), move it into the 1-element Option whose scope drop routes to the
    /// RECURSIVE `$__drop_opt_str_int` (`variant_drop_handles = "opt_str_int"`, B41) —
    /// the flat `DropListStr` a bare `is_heap_ty` fallback would use only frees the
    /// TUPLE's own refcount, leaking its String (the same class of bug B41's DIAGNOSIS
    /// caught for the BIND position; this is its ARM-position mirror in
    /// `try_lower_option_ctor`, binds_p4.rs, never ported here). Checked BEFORE the
    /// generic `is_heap_ty` fallback, which has no `Tuple` case at all.
    fn arm_some_str_scalar_tuple(&mut self, expr: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let repr = repr_of(result_ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else { return None };
        let elements = elements.clone();
        let piece = self.try_lower_tuple_construct(&elements)?;
        let obj = self.materialize_opt_str_some(piece, repr);
        self.variant_drop_handles.insert(obj, "opt_str_int".to_string());
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
            
            
    }

    /// The OWNED heap payload a general `some(<heap>)` arm moves into the Option.
    fn arm_some_heap_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        Some(match &expr.kind {
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
            // `some(string.slice(s, …))` / `some(list.drop_end(stack, 1))` — a PURE
            // Module call yielding a fresh owned HEAP payload (String, or the fold-step's
            // List[String]): the self-host call's result moves into the Option
            // (retain-removed — the Option is the sole owner); its arg temps free within
            // the arm frame below. The moved-in payload's recursive free is the CALLER's
            // (per the option's bind-site drop routing), not this arm's.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if is_heap_ty(&expr.ty) =>
            {
                // Arg temps this call materializes must free WITHIN the arm — a per-arm
                // temp left to the FUNCTION epilogue would rc_dec an UNINITIALIZED local
                // when the OTHER arm ran (garbage rc_dec → trap; the fold-step `["("]`
                // concat temp reproduced exactly this).
                let arm_mark = self.live_heap_handles.len();
                let p = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty)
                    .ok()?;
                self.live_heap_handles.retain(|h| *h != p);
                self.drop_arm_locals(arm_mark);
                p
            }
            // `some(stack + ["("])` — the fold-step push: a fresh owned concat list
            // moves into the Option directly (no Dup — sole owner). The concat's
            // materialized RHS-element temp frees WITHIN the arm (same trap avoidance
            // as the Module-call case above).
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                let arm_mark = self.live_heap_handles.len();
                let p = self.try_lower_concat_list(expr)?;
                self.live_heap_handles.retain(|h| *h != p);
                self.drop_arm_locals(arm_mark);
                p
            }
            // `some("café")` — a String LITERAL payload. The bare-payload arm
            // right above already lowers a literal this way (`Init::Str` into a
            // fresh owned block); wrapping it in `Some` needs the same alloc, then
            // the move into the Option. Missing it walled the whole enclosing
            // function — `let v = ok(if c then some("ΑΒΓ") else some("café"))`,
            // the archived café fuzz finding, is exactly two literal Some arms.
            // The block is fresh and rc 1, so it moves in with no Dup.
            IrExprKind::LitStr { value } => {
                let pr = repr_of(&expr.ty).ok()?;
                let p = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst: p,
                    repr: pr,
                    init: Init::Str(value.clone()),
                });
                p
            }
            _ => return None,
        })
    }
}
