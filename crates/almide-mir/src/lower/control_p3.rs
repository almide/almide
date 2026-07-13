impl LowerCtx {
    /// Try to EXECUTE `<materialized Option> ?? <scalar fallback>` to a SCALAR value: read
    /// the tag (len) and yield the payload (`data[0]`) when Some, else the fallback. Gated
    /// to a DIRECT self-host Option call — every such fn returns `Option[Int]`, so the
    /// payload is a scalar (no element alias), and its result is a real materialized Option
    /// dropped at scope end. Returns the scalar `dst`, or `None` (rolled back) when not in
    /// this subset (a non-call expr, or a heap fallback) — the caller defers to `Opaque`.
    ///
    /// SOUND: the Option's `Alloc` (the now-MATERIALIZED call, no longer elided) is `i`,
    /// dropped at scope end `d` = balanced; the tag/payload reads are scalar prims, the
    /// markers no-op, the payload is an i64 value COPY (not an alias), so dropping the
    /// Option after is safe. The call becoming real only improves caps (analyzed, not
    /// elided) and stays 1:1 with its IR call-node (no mir>ir issue).
    /// `track_result` governs the HEAP-String result only: `true` (a let-bind / call-arg temp)
    /// pushes the fresh owned String to `live_heap_handles` so it is dropped at scope end; `false`
    /// (a RETURN/tail position) leaves it untracked because it is MOVED OUT to the caller (tracking
    /// it would double-free). The scalar path is unaffected (a scalar result owns nothing).
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // An `Option[record]` operand (`list.get(tools, i) ?? { name: "", … }`) has NO faithful
        // `??` lowering yet: the Value-shaped `option.value_unwrap_or` corrupts a record field
        // block (both arms printed garbage / empty fields vs v0), and no other path here handles
        // it. DECLINE outright so the whole `??` walls cleanly (never a wrong byte) — a correct
        // record-payload unwrap-or is a follow-up. Gated to a record/anon-record Option payload.
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &expr.ty
        {
            if a.len() == 1 && self.record_or_anon_drop_type_name(&a[0]).is_some() {
                return None;
            }
        }
        // The Option operand's handle: either a VAR already bound to a materialized Option
        // (`let o = list.get(xs, i); o ?? d` — the most common form, BORROWED, dropped by its
        // own let-bind at scope end), a function PARAM of Option type (`fn f(o: Option[String]) =
        // o ?? d` — passed by the caller as a real materialized Option block, BORROWED, not dropped
        // here), or a DIRECT self-host Option call (materialized here). The param case is sound by
        // the same evidence as `materialized_options`: an Option-typed param is a real 0-or-1-
        // element block (the calling convention), so its len-as-tag read is correct — NOT a deferred
        // Opaque (those are never params), which is why the bare-Var gate excludes non-Option Vars.
        //
        // A `??` operand is EITHER an Option (`o ?? d` → Some-payload / fallback) OR a scalar Result
        // (`int.parse(s) ?? -1` → Ok-payload / fallback). They share the len-as-tag layout but read
        // INVERSELY: Option Some = `tag != 0` (take payload), Result Ok = `tag == 0` (take payload).
        // `is_result` selects the arm arrangement below; a Result operand also skips the Option-only
        // `option.unwrap_or_str` String branch (a `Result[String,String] ?? "d"` is a later case).
        let is_named_variant_call = matches!(
            &expr.kind,
            IrExprKind::Call { target: CallTarget::Named { .. }, .. }
        ) && is_variant_ty(&expr.ty);
        let is_result = match &expr.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .ok()
                .map(|v| {
                    self.materialized_results.contains(&v)
                        && !self.materialized_options.contains(&v)
                })
                .unwrap_or(false),
            // A USER function returning Result — read its tag INVERSELY (Ok = tag 0).
            _ if is_named_variant_call => is_result_ty(&expr.ty),
            _ => is_self_host_result_call(expr),
        };
        let handle = if let IrExprKind::Var { id } = &expr.kind {
            match self.value_for(*id) {
                // A bare-Var operand must be a tracked materialized Option/Result OR a borrowed
                // variant PARAM (`param_values` — same calling-convention soundness as the match):
                // a deferred Opaque Var (len 0) would MISREAD as None/Err, so it is excluded.
                Ok(v)
                    if self.materialized_options.contains(&v)
                        || self.materialized_results.contains(&v)
                        // a Value/List-Ok Result Var (`value.get`/`value.as_array` result) — its `??`
                        // routes to the value_unwrap helper below. A String-Ok Result (heap_elem_lists)
                        // is NOT admitted here: it keeps its original path (the String branch is for
                        // OPTION[String], and counting a str-Result there falsely taints mir>ir).
                        || self.value_result_results.contains(&v)
                        || self.value_result_lists.contains(&v)
                        // A `Result[String, String]` Var (cap-as-tag, materialized_results_str)
                        // routes to the `result.str_unwrap_or` helper below — admitted ONLY for
                        // that type (any other _str-set shape would mis-take the len-as-tag
                        // String branch, reading an Err payload as Some).
                        || (self.materialized_results_str.contains(&v)
                            && crate::lower::is_result_str_str_ty(&expr.ty))
                        || self.param_values.contains(&v) =>
                {
                    v
                }
                _ => return None,
            }
        } else if let IrExprKind::Member { object, field } = &expr.kind {
            // `r.opt ?? d` — an `Option[scalar/String]` FIELD of a materialized record: BORROW the
            // field's Option block handle (`LoadHandle` at the field offset; the record keeps
            // ownership, the `??` only READS the tag + scalar/String payload — no transfer, no drop).
            // Gated to a scalar/String Option leaf so the scalar-payload / `option.unwrap_or_str` read
            // below is over a real 0-or-1 block. Exposed by derived-Codec Option decode (the
            // codec_float_int `r.opt ?? -1.0` consumer): without it the `??` fell to a silent Const-0.
            let leaf_ok = matches!(&expr.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Float | Ty::Bool | Ty::String));
            if !leaf_ok {
                return None;
            }
            let offset = match self.aggregate_field_offset_any(&object.ty, field.as_str()) {
                Some(o) => o,
                None => return None,
            };
            let ch = match self.resolve_aggregate_container_handle(object) {
                Some(c) => c,
                None => return None,
            };
            self.load_at_offset(ch, offset as i64, PrimKind::LoadHandle)
        } else if is_self_host_option_call(expr)
            || is_self_host_result_call(expr)
            || (is_self_host_result_str_call(expr)
                && (crate::lower::is_value_result_ty(&expr.ty)
                    || crate::lower::is_result_listval_ty(&expr.ty)
                    || crate::lower::is_result_str_str_ty(&expr.ty)))
            || is_named_variant_call
        {
            // A self-host OR user-function call returning Option/Result — materialize it (the
            // Named-call arm seeds the READ-shape into `materialized_options/results`, so the
            // tag read below is over a KNOWN-layout block) and read its tag, exactly like a
            // tracked Var. The owned result is dropped at scope end by `materialized_call_arg`.
            match self.lower_call_args(std::slice::from_ref(expr)) {
                Ok(args) => match args.into_iter().next() {
                    Some(CallArg::Handle(v)) => v,
                    _ => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                },
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        } else if let Some(v) = self.materialize_unwrap_or_operand(expr) {
            // A `??` over a variant-returning call NOT in the self-host registries — the
            // `json.parse(s) ?? d` (PURE heap-Result) / `process.env(k) ?? d` (IMPURE intrinsic
            // Option[String]) class. `materialize_unwrap_or_operand` routes it through the SAME
            // proven machinery a recognized self-host operand uses (the recursive drop is
            // registered by type), so the helper read below (`option.unwrap_or_str` /
            // `result.value_unwrap_or`) is over a real owned block, freed once at scope end.
            v
        } else {
            return None;
        };
        // HEAP-Value Result `??` (`value.get(o,k) ?? value.null()` — Result[Value,String]): route to
        // the self-hosted `result.value_unwrap_or`, which reuses the Value-Ok match read (its
        // `ok(v) => v` arm Dup's the @12 payload; the Result is freed by its scope-end DropResultValue).
        // A call returning a FRESH owned Value sidesteps the bind-position heap-result rc bookkeeping,
        // exactly like `option.unwrap_or_str` for the String payload.
        // The Ok payload selects the helper: a single Value (`value.get`) → `result.value_unwrap_or`;
        // a `List[Value]` (`value.as_array`) → `result.list_value_unwrap_or` (recursive list drop).
        // Both reuse the working heap-Ok match read; a call returning a fresh owned heap value
        // sidesteps the bind-position rc bookkeeping, like `option.unwrap_or_str` for the String case.
        let value_unwrap_helper = if crate::lower::is_result_listval_ty(&expr.ty) {
            Some("result.list_value_unwrap_or")
        } else if crate::lower::is_value_result_ty(&expr.ty) {
            Some("result.value_unwrap_or")
        } else if crate::lower::is_result_str_str_ty(&expr.ty) {
            Some("result.str_unwrap_or")
        } else if crate::lower::is_option_value_ty(&expr.ty) {
            Some("option.value_unwrap_or")
        } else if crate::lower::is_option_liststr_ty(&expr.ty) {
            Some("option.liststr_unwrap_or")
        } else if crate::lower::is_option_listvalue_ty(&expr.ty) {
            Some("option.listvalue_unwrap_or")
        } else {
            // An `Option[record]` payload (`list.get(tools, i) ?? { name: "", … }`) is NOT
            // routed to `option.value_unwrap_or`: that helper does a VALUE-shaped handle select
            // (it reads a tagged Value block), which CORRUPTS a plain record field block — BOTH
            // the Some and the None arm printed garbage (0x18 0x20…) vs v0's real field (a
            // pre-existing miscompile the mir>ir gate flagged on porta parse_manifest). Decline
            // here so the `??` walls cleanly; a correct record-payload unwrap-or is a follow-up.
            None
        };
        if let Some(helper) = value_unwrap_helper {
            if is_heap_ty(&fallback.ty) {
                let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
                    Ok(a) => a,
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                };
                let repr = match repr_of(&expr.ty) {
                    Ok(r) => r,
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                };
                let mut call_args = vec![CallArg::Handle(handle)];
                call_args.extend(fb_args);
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: helper.to_string(),
                    args: call_args,
                    result: Some(repr),
                });
                if track_result {
                    self.live_heap_handles.push(dst);
                }
                return Some(dst);
            }
        }
        // HEAP-String result (`Option[String] ?? "default"` — the most common heap `??`): the scalar
        // unwrap below can't carry a heap payload (it would mis-read the slot-0 String HANDLE as an
        // i64 scalar). Route to the self-host `option.unwrap_or_str` CALL — a call returning a FRESH
        // owned String (cert `i`, bound + dropped like any heap value), which sidesteps the
        // bind-position heap-result-`if` cert problem entirely. The Option is BORROWED (the callee
        // reads + copies it); the fallback is materialized/borrowed by `lower_call_args`. Gated to
        // `Ty::String` (a `List`/other-heap payload would corrupt — its slot is not a String handle),
        // and `count_ir_calls` counts a String-fallback `UnwrapOr` node so this synthetic call keeps
        // `mir_calls <= ir_calls` (the same accounting as the `__str_concat` operator-desugar).
        if matches!(fallback.ty, Ty::String) && !is_result {
            let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
                Ok(a) => a,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let repr = match repr_of(&fallback.ty) {
                Ok(r) => r,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let mut call_args = vec![CallArg::Handle(handle)];
            call_args.extend(fb_args);
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: "option.unwrap_or_str".to_string(),
                args: call_args,
                result: Some(repr),
            });
            if track_result {
                self.live_heap_handles.push(dst);
            }
            return Some(dst);
        }
        // A SCALAR `??`: read the tag (len @4) and pick the slot-0 payload vs the fallback. The
        // payload is an i64 value COPY (`Load width 8`) — fine for a scalar Ok/Some; a heap payload
        // is handled by the String branch above (Option) or stays out of subset (Result[String,…]).
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        // `IfThen` runs the THEN arm when `tag != 0`. For an OPTION that is Some (take the slot-0
        // payload); for a RESULT that is Err (take the FALLBACK — Ok is `tag == 0`, the ELSE arm).
        // So the two arms are SWAPPED between the cases. The ops emitted between IfThen/Else land in
        // the THEN body, those between Else/EndIf in the ELSE body — so the payload Load and the
        // fallback computation must each sit in the arm that USES them.
        if is_result {
            // THEN = Err (tag != 0) → the fallback computed HERE; ELSE = Ok → the slot-0 payload.
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::Else { val: Some(fb) });
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::EndIf { val: Some(payload) });
        } else {
            // THEN = Some (tag != 0) → the slot-0 payload loaded HERE; ELSE = None → the fallback.
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::Else { val: Some(payload) });
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::EndIf { val: Some(fb) });
        }
        Some(result)
    }

    /// Materialize a `??` OPERAND that is an Option/Result-returning `Module`/`RuntimeCall` NOT in
    /// the self-host registries — the `json.parse(s) ?? d` (PURE heap-Result) and the
    /// `process.env(k) ?? "/tmp"` (IMPURE intrinsic `Option[String]`) class. This is the SAME
    /// operand a recognized self-host call (`value.get` / `json.as_array`) would be; only its
    /// callee was unrecognized, so the `??` walled instead of executing.
    ///
    /// - A PURE `Module` variant call routes through the standard call-arg machinery
    ///   (`lower_call_args` → `materialized_call_arg`), which registers the recursive drop set BY
    ///   TYPE — byte-identical ownership to materializing `value.get`. `json.parse` (pure, in
    ///   `PURE_MODULES`) is exactly this; the `result.value_unwrap_or` read below is over a real
    ///   `Result[Value, String]` block freed once at scope end.
    /// - An IMPURE intrinsic `Option[String]` call (`process.env`) routes through the proven
    ///   effect-subject materialization (`try_materialize_effect_result_subject` — the #76
    ///   direct-`CallFn` path) and registers the FLAT `DropListStr` drop (`heap_elem_lists`):
    ///   exact for a 0-or-1-element String Option. The `option.unwrap_or_str` read below borrows it.
    ///
    /// HOLE-1 DISCIPLINE: the impure path is gated STRICTLY to `Option[String]` — the only variant
    /// whose payload `DropListStr` frees exactly. A record / aggregate / `Value`-Ok impure operand
    /// has no proven flat drop here, so it is REFUSED (returns None) and the caller walls cleanly
    /// rather than register a leaky flat cert. Returns the OWNED operand block (in
    /// `live_heap_handles`), or None (rolled back) when out of subset.
    fn materialize_unwrap_or_operand(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        // The same structural gate the caps counter consults (no count drift).
        if !unwrap_or_operand_admitted(expr) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // PURE Option/Result `Module` call (`json.parse` / `toml.parse`): the standard call-arg
        // machinery materializes it with the correct recursive drop registration, exactly like a
        // recognized self-host operand. (A self-host-recognized call never reaches here — it was
        // matched in the gate above — so this is only the unrecognized pure remainder.)
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &expr.kind {
            if crate::purity::is_pure(module.as_str(), func.as_str()) {
                return match self.lower_call_args(std::slice::from_ref(expr)) {
                    Ok(args) => match args.into_iter().next() {
                        Some(CallArg::Handle(v)) => Some(v),
                        _ => rollback(self),
                    },
                    Err(_) => rollback(self),
                };
            }
        }
        // IMPURE intrinsic `Option[String]` (`process.env`): admit ONLY this shape — its DynListStr
        // 0-or-1-element String drop is `DropListStr` exactly (`heap_elem_lists`). Any other impure
        // variant (a `Value`/record/list Ok, a `Result`) has no proven flat drop here → REFUSE.
        let impure_admitted = matches!(
            &expr.ty,
            Ty::Applied(TC::Option, a) if a.len() == 1 && matches!(a[0], Ty::String)
        );
        if !impure_admitted {
            return None;
        }
        let dst = match self.try_materialize_effect_result_subject(expr) {
            Some(v) => v,
            None => return rollback(self),
        };
        // The 0-or-1-element String Option is freed recursively by `DropListStr` (frees the slot-0
        // String, if present, then the block). `register_owned_heap_eq_drop` inserts `heap_elem_lists`
        // for this `is_heap_elem_list_ty` shape — exact, never a leak.
        self.register_owned_heap_eq_drop(dst, &expr.ty);
        Some(dst)
    }

    /// Emit `base + offset` then a `prim` load of `kind` at that address, returning the
    /// loaded value (an i64 in the prim floor's uniform model). The address arithmetic
    /// mirrors what `prim.handle(x) + offset` lowers to (`Op::ConstInt` + `Op::IntBinOp`).
    pub(crate) fn load_at_offset(&mut self, base: ValueId, offset: i64, kind: crate::PrimKind) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![addr] });
        dst
    }

    /// Lower ONE scalar `if` arm (a block's statements + a scalar tail value) with a
    /// per-arm scope frame: the heap temps the arm allocates are dropped WITHIN the arm
    /// (so taken-arm-only execution stays balanced). Returns the tail's scalar value.
    pub(crate) fn lower_scalar_arm(&mut self, arm: &IrExpr) -> Option<ValueId> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &arm.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(arm)),
        };
        let lhh_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in stmts {
            if self.lower_stmt(stmt).is_err() {
                self.in_frame -= 1;
                return None;
            }
        }
        // A nested `if`/`match` tail (an else-if chain — what a desugared `match`
        // produces) EXECUTES recursively via the scalar-if machinery; otherwise the
        // tail is a scalar value or a scalar call.
        let val = tail.and_then(|t| match &t.kind {
            IrExprKind::If { cond, then, else_ } => {
                self.try_lower_scalar_if(cond, then, else_, &t.ty)
            }
            // A nested VARIANT (Option/Result) match (`err(_) => match float.parse(s) { … }` —
            // the is_numeric_or_bool / looks_numeric chain) EXECUTES via the same tag-read
            // value-match the tail uses; its own arms recurse through `lower_scalar_arm`, so an
            // N-deep nest lowers. A LITERAL-pattern match desugars to the if-chain as before.
            // A nested CUSTOM-variant (user ADT) match (`A(b) => match b { … }` — the
            // mutual-recursive-types depth walk): dispatch on its tag exactly like the
            // tail does. Checked BEFORE the Option/Result variant path (a custom ADT is
            // not an Option/Result). Without this the nested match fell to the deferred
            // Const-0 (a silent miscompile).
            IrExprKind::Match { subject, arms }
                if self.custom_variant_type_name(&subject.ty).is_some() =>
            {
                self.try_lower_custom_variant_match(subject, arms, &t.ty)
            }
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                self.try_lower_variant_value_match(subject, arms, &t.ty)
            }
            IrExprKind::Match { subject, arms } => self
                .desugar_match_to_if(subject, arms, &t.ty)
                .and_then(|if_expr| match &if_expr.kind {
                    IrExprKind::If { cond, then, else_ } => {
                        self.try_lower_scalar_if(cond, then, else_, &t.ty)
                    }
                    _ => None,
                }),
            // A SCALAR-typed `Try`/`Unwrap` tail over a never-err effect call (`else { … loop(n-1, acc) }`
            // — the recursive value-accumulator: the frontend wraps the propagating effect call in
            // `Try` for the auto-`?`, and the call-type unwrap pass already made its `.ty` the raw scalar
            // `T`). The `?`/`!` over a never-err call is a no-op (it always returns Ok), so strip it and
            // lower the inner call as the scalar value. WITHOUT this the `Try`/`Unwrap` fell to the
            // `_ => lower_scalar_value | try_lower_scalar_call` arm (neither handles a `Try`/`Unwrap`
            // node) → the arm FAILED → the scalar-`if` rolled back → the whole body collapsed to a
            // deferred Const (the recursive accumulator returned a garbage 0). The scalar-arm twin of
            // the statement/tail effect-`Try` fix.
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } if !is_heap_ty(&t.ty) => {
                self.lower_scalar_value(expr).or_else(|| self.try_lower_scalar_call(expr, &expr.ty))
            }
            _ => self.lower_scalar_value(t).or_else(|| self.try_lower_scalar_call(t, &t.ty)),
        });
        self.in_frame -= 1;
        self.drop_arm_locals(lhh_mark);
        val
    }

    /// Drop every heap handle the current scope frame added beyond `mark` (LIFO),
    /// restoring `live_heap_handles` to its pre-frame length — the per-arm teardown.
    pub(crate) fn drop_arm_locals(&mut self, mark: usize) {
        // Release the handles created SINCE `mark`. Clamp to the current length: a sub-lowering
        // (e.g. a match that drops its own subject) can leave `live_heap_handles` SHORTER than
        // `mark`, in which case nothing new is live and this is a no-op (a bare `split_off(mark)`
        // would panic). Semantically "drop everything from mark onward, nothing if none".
        let mark = mark.min(self.live_heap_handles.len());
        for v in self.live_heap_handles.split_off(mark).into_iter().rev() {
            let op = self.drop_op_for(v);
            self.ops.push(op);
        }
    }

    /// Lower a `for v in iterable { body }` by modeling ONE iteration with a
    /// PER-ITERATION SCOPE FRAME. Each iteration is internally balanced (its loop
    /// variable + body locals are all dropped at iteration end), so N runtime
    /// iterations are N balanced episodes — no cross-iteration leak or double-free,
    /// and the flat cert (one iteration) is sound for any N (including 0: every op is
    /// in a balanced frame). NO loop op — the iteration discipline lives entirely in
    /// the lowering, the checker stays a flat fold.
    ///
    /// The ITERABLE is evaluated once: a heap iterable is lowered by `lower_call_args`
    /// — an already-tracked `Var` is BORROWED, a FRESH heap value (a call/literal
    /// result) is MATERIALIZED into an owned temp released at the OUTER scope; a scalar
    /// iterable (a `Range`) carries no ownership. The LOOP VARIABLE binds one element per
    /// iteration: a HEAP element ALIASES the whole container (`Op::Dup`, container-
    /// grain like field extraction — it keeps the container alive for the iteration,
    /// dropped at its end; element-precise identity needs the layout brick), a SCALAR
    /// element is a `Const`. A `break`/`continue` is a no-op admitted ONLY over a
    /// SCALAR-only frame (`wall_break_over_heap_frame`); over a heap frame it is WALLED
    /// (a real early exit would skip a per-iteration heap Drop = a wasm leak). A HEAP
    /// reassignment (the accumulator, `acc = acc + [x]`) is DEFERRED, not walled: the
    /// `in_frame` discipline keeps `acc` pinned to its still-live handle across
    /// iterations (memory-safe; the accumulation is deferred like every `Opaque`) and it
    /// is not a frame handle. A scalar reassignment (`i = i + 1`) is a Copy `Const`,
    /// harmless, admitted.
    pub(crate) fn lower_for_in(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), LowerError> {
        // First try to EXECUTE a scalar `for i in start..end` as a real loop; out of that
        // subset it rolls back and we keep the model-one-iteration form below.
        if self.try_lower_scalar_for_range(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // Then try to EXECUTE `for x in xs` over a List[Int] as a real element loop.
        if self.try_lower_scalar_for_list(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // The iterable is evaluated ONCE before the loop. A heap iterable goes through
        // `lower_call_args` — an already-tracked `Var` is borrowed (no new ownership),
        // a fresh heap value is materialized into an owned temp dropped at the OUTER
        // scope (its caps captured by the lowering). A scalar iterable (a `Range`)
        // carries no ownership; capture any call in it for caps.
        let container: Option<ValueId> = if is_heap_ty(&iterable.ty)
            // A Range ITERABLE stays on the no-container model path (it carries no
            // ownership; the call-arg Range materialization emits a `list.range` CallFn
            // the caps ledger only accounts for in ARGUMENT position).
            && !matches!(&iterable.kind, IrExprKind::Range { .. })
        {
            match self.lower_call_args(std::slice::from_ref(iterable))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(iterable);
            None
        };
        let mark = self.live_heap_handles.len();
        let vars: Vec<VarId> = match var_tuple {
            Some(vs) => vs.clone(),
            None => vec![var],
        };
        for v in vars {
            // A heap element aliases the whole container; a scalar element is a Const.
            let elem_ty = find_var_ty(body, v);
            let elem_heap = elem_ty.as_ref().map(|t| is_heap_ty(t)).unwrap_or(false);
            if elem_heap {
                // The model-one-iteration fallback aliases the element to the WHOLE container (a `Dup`
                // of the list handle) — the real per-element loop (`try_lower_scalar_for_list`) only
                // covers List[scalar]/List[String], which never reach here. A heap-AGGREGATE element
                // (tuple/record) DOES reach here, and reading a field of it (`for p in ps { p.0 }`)
                // would read the CONTAINER's slot, not the element's — a SILENT MISCOMPILE. Now that
                // the producer side materializes List[tuple]/List[record] returns, that consumer is
                // reachable, so WALL it (honest) until the heap-aggregate per-element borrow lands.
                if elem_ty.as_ref().is_some_and(|t| self.aggregate_field_tys(t).is_some())
                    && body_reads_var_field(body, v)
                {
                    return Err(LowerError::Unsupported(
                        "for-in over a List of heap aggregates (tuple/record) whose element is read \
                         via a direct field/index (`for p in ps { p.0 }`) is not in this brick: the \
                         model-one-iteration fallback aliases the element to the whole container, so \
                         the projection would read the container — walled to avoid a silent \
                         miscompile (needs the heap-aggregate per-element borrow)"
                            .into(),
                    ));
                }
                let src = container.ok_or_else(|| {
                    LowerError::Unsupported(
                        "for-in heap loop variable over a non-container iterable not in this brick".into(),
                    )
                })?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.value_of.insert(v, dst);
                self.live_heap_handles.push(dst);
            } else {
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("for-in loop variable"));
                }
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.value_of.insert(v, dst);
            }
        }
        // A heap reassignment in the body is the loop ACCUMULATOR (`acc = acc + [x]`):
        // it is DEFERRED, not rebound (the `in_frame` discipline) — `acc` keeps its
        // still-live handle across iterations, so the next iteration never dereferences
        // a freed handle. Memory-safe; the accumulation itself is deferred like `Opaque`.
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.wall_break_over_heap_frame(body, "for-in", mark)?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Lower a `while cond { body }` like a `for-in` body — a PER-ITERATION SCOPE
    /// FRAME makes one modeled iteration balanced, sound for any N. The condition is
    /// evaluated each iteration (its caps captured); the body's locals are dropped at
    /// iteration end. Same as `for-in`: a `break`/`continue` over a HEAP frame is walled
    /// (post-lowering), a heap reassignment (accumulator) deferred by `in_frame`.
    /// Try to lower `while cond { body }` as a REAL scalar-state loop that EXECUTES N
    /// times (the `LoopStart`/`LoopBreakUnless`/`LoopEnd` markers), reassigning scalar
    /// loop-carried state via [`Op::SetLocal`]. Restricted to the sound + runnable subset:
    /// an Int/Bool cond, NO `break`/`continue` (a no-op early-exit would be wrong inside a
    /// real loop), and a body with NO heap reassignment (the `scalar_loop_depth` Assign
    /// rule errors on one) and NO net heap handle escaping the per-iteration frame. The
    /// cond ops sit INSIDE the loop (re-evaluated each iteration); per-iteration heap (a
    /// string literal in `println`) is dropped before the back-edge. SOUNDNESS by REUSE:
    /// the markers are no-ops in verify_ownership and the body is a per-iteration-balanced
    /// frame — the cert verifies ONE balanced iteration, sound for any N (the existing
    /// model-one-iteration argument), the markers only make wasm actually run it N times.
    /// Returns false (and rolls back) when out of subset; `lower_while` then falls back.
    pub(crate) fn try_lower_scalar_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> bool {
        if !matches!(cond.ty, Ty::Int | Ty::Bool) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        self.ops.push(Op::LoopStart);
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                return false;
            }
        };
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_while_body_stmt(stmt).is_err() {
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
        // Per-iteration heap (a string literal in a body `println`) is released before the
        // back-edge, INSIDE the loop, so each iteration is balanced.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::LoopEnd);
        true
    }

    /// One while-body statement, admitting the CONDITIONAL-BREAK forms the real loop
    /// can execute with the EXISTING marker vocabulary (no new op):
    ///   `if c then <rest> else break`  (the guard-else-break desugar) →
    ///       `LoopBreakUnless(c)` then <rest> emitted linearly — on the broken path the
    ///       `br` already exited, exactly like the loop-head condition;
    ///   `if c then break else ()`      (the do-block shape) →
    ///       `LoopBreakUnless(1 - c)`.
    /// Any OTHER Break/Continue in the statement ERRS (aborting the attempt → the
    /// model-one-iteration fallback then WALLS): `lower_stmt` silently swallows a bare
    /// Break (mod_p3), so delegating one would silently drop the early exit.
    /// `{ A…; break }` → `Some({ A… })` — the then-arm of a mid-body conditional break
    /// with statements BEFORE the break. `None` when the expr does not end in a bare
    /// trailing break (or has nothing before it — the simpler cases own that shape).
    fn strip_trailing_break_expr(e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Block { stmts, expr } = &e.kind else { return None };
        // Trailing break as the block TAIL.
        if let Some(t) = expr.as_deref() {
            if matches!(t.kind, IrExprKind::Break) && !stmts.is_empty() {
                return Some(IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: None },
                    ty: almide_lang::types::Ty::Unit,
                    span: e.span.clone(),
                    def_id: e.def_id,
                });
            }
            return None;
        }
        // Trailing break as the LAST statement.
        if stmts.len() >= 2 {
            if let IrStmtKind::Expr { expr: last } = &stmts[stmts.len() - 1].kind {
                if matches!(last.kind, IrExprKind::Break) {
                    return Some(IrExpr {
                        kind: IrExprKind::Block {
                            stmts: stmts[..stmts.len() - 1].to_vec(),
                            expr: None,
                        },
                        ty: almide_lang::types::Ty::Unit,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    });
                }
            }
        }
        None
    }

    fn lower_while_body_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        fn is_break(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Break => true,
                IrExprKind::Block { stmts, expr } => {
                    (stmts.is_empty() && expr.as_deref().is_some_and(is_break))
                        || (expr.is_none()
                            && stmts.len() == 1
                            && matches!(&stmts[0].kind,
                                IrStmtKind::Expr { expr } if is_break(expr)))
                }
                _ => false,
            }
        }
        fn is_unit(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Unit => true,
                IrExprKind::Block { stmts, expr } => {
                    stmts.is_empty() && expr.as_deref().map_or(true, is_unit)
                }
                _ => false,
            }
        }
        if let IrStmtKind::Expr { expr } = &stmt.kind {
            // A BARE break statement (`if true then break else ()` const-folds to it):
            // break unconditionally — LoopBreakUnless over a 0 cond.
            if matches!(expr.kind, IrExprKind::Break) {
                let z = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: z, value: 0 });
                self.ops.push(Op::LoopBreakUnless { cond: z });
                return Ok(());
            }
            if let IrExprKind::If { cond, then, else_ } = &expr.kind {
                if is_break(else_) {
                    let c = self.lower_scalar_value(cond).ok_or_else(|| {
                        LowerError::Unsupported("while conditional-break cond".into())
                    })?;
                    self.ops.push(Op::LoopBreakUnless { cond: c });
                    return self.lower_while_body_inline(then);
                }
                if is_break(then) && is_unit(else_) {
                    let c = self.lower_scalar_value(cond).ok_or_else(|| {
                        LowerError::Unsupported("while conditional-break cond".into())
                    })?;
                    let one = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: one, value: 1 });
                    let nc = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: nc, op: IntOp::Sub, a: one, b: c });
                    self.ops.push(Op::LoopBreakUnless { cond: nc });
                    return Ok(());
                }
                // `if c then { A…; break } else B` (find_factor: `if n % i == 0 then
                // { result = i; break } else { i = i + 1 }`): CAPTURE the (call-free,
                // pure-scalar) cond once, run the ordinary unit `if` with the trailing
                // break STRIPPED (both arms then break-free — the statement-if machinery
                // branches the arm assigns correctly), and break on the CAPTURED value
                // after. The capture keeps the break test the value the branch dispatched
                // on even when an arm mutates the cond's operands (`i = i + 1`).
                if let Some(then_stripped) = Self::strip_trailing_break_expr(then) {
                    if !body_breaks_or_continues(std::slice::from_ref(&IrStmt {
                        kind: IrStmtKind::Expr { expr: then_stripped.clone() },
                        span: stmt.span.clone(),
                    })) && !body_breaks_or_continues(std::slice::from_ref(&IrStmt {
                        kind: IrStmtKind::Expr { expr: (**else_).clone() },
                        span: stmt.span.clone(),
                    })) && !crate::lower::expr_contains_call(cond)
                    {
                        let c = self.lower_scalar_value(cond).ok_or_else(|| {
                            LowerError::Unsupported("while conditional-break cond".into())
                        })?;
                        let unit_if = IrStmt {
                            kind: IrStmtKind::Expr {
                                expr: IrExpr {
                                    kind: IrExprKind::If {
                                        cond: cond.clone(),
                                        then: Box::new(then_stripped),
                                        else_: else_.clone(),
                                    },
                                    ty: almide_lang::types::Ty::Unit,
                                    span: expr.span.clone(),
                                    def_id: expr.def_id,
                                },
                            },
                            span: stmt.span.clone(),
                        };
                        self.lower_stmt(&unit_if)?;
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        let nc = self.fresh_value();
                        self.ops.push(Op::IntBinOp { dst: nc, op: IntOp::Sub, a: one, b: c });
                        self.ops.push(Op::LoopBreakUnless { cond: nc });
                        return Ok(());
                    }
                }
            }
        }
        if body_breaks_or_continues(std::slice::from_ref(stmt)) {
            return Err(LowerError::Unsupported(
                "unrecognized break/continue in a while body (lower_stmt would silently \
                 swallow it) not in this brick"
                    .into(),
            ));
        }
        self.lower_stmt(stmt)
    }

    /// Inline the surviving arm of a while conditional-break (`if c then <rest> else
    /// break`): a Block's statements re-enter [`Self::lower_while_body_stmt`] (a nested
    /// conditional break chains), a unit tail is nothing, any other tail lowers as an
    /// effect statement.
    fn lower_while_body_inline(&mut self, e: &IrExpr) -> Result<(), LowerError> {
        match &e.kind {
            IrExprKind::Unit => Ok(()),
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_while_body_stmt(s)?;
                }
                match expr.as_deref() {
                    Some(t) => self.lower_while_body_inline(t),
                    None => Ok(()),
                }
            }
            _ => self.lower_while_body_stmt(&IrStmt {
                kind: IrStmtKind::Expr { expr: e.clone() },
                span: e.span.clone(),
            }),
        }
    }

    /// Roll back a scalar-loop ATTEMPT (`try_lower_scalar_while` / `_for_range` / `_for_list`),
    /// restoring EVERY side-effect the partial body lowering may have produced — not only `ops`
    /// but the LAMBDA-LIFTED auxiliaries (`self.lifted`). A lambda call-arg in the body (`for x in
    /// xs { … list.map([y], (u) => …) … }`) lifts a `__lambda_*` MirFunction into `self.lifted`;
    /// if the attempt then rolls back (a heap reassignment aborts it → the model-one-iteration
    /// fallback re-lowers the SAME body, re-lifting the lambda), the abandoned first copy would
    /// survive and DOUBLE-COUNT its inner calls (a `mir > ir` wall breach). Truncating `lifted` to
    /// `lifted_mark` (captured at THIS attempt's start, threaded as a local so NESTED loop attempts
    /// each roll back to their own floor) makes the rollback total.
    fn rollback_scalar_loop(
        &mut self,
        ops_mark: usize,
        lhh_mark: usize,
        lifted_mark: usize,
        value_of_snapshot: std::collections::HashMap<almide_ir::VarId, ValueId>,
    ) {
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        self.lifted.truncate(lifted_mark);
        self.value_of = value_of_snapshot;
    }

    /// Try to lower a HEAP-RESULT `if cond then A else B` (a String/data-returning branch)
    /// to EXECUTABLE control flow — only the taken arm allocates, and its value is the
    /// function result. SOUNDNESS by PER-ARM BALANCE (no Coq change — see
    /// docs/roadmap/active/v1-heap-result-control-flow.md): each arm `Alloc`s its value
    /// (cert `i`) AND `Consume`s it (cert `m`) so the arm is internally `"im"` balanced
    /// exactly like a scalar arm carries none; the `IfThen` result `dst` is NEVER an
    /// `Alloc`, so it is not in the ownership cert's object set and `func.ret = dst` emits
    /// no second move-out (no double-free). The render selects one arm at runtime
    /// (`(if (result i32) …)`), so exactly one `Alloc` happens and is returned rc=1 to the
    /// caller — the untaken arm never allocates (no leak). FIRST version: both arms are
    /// direct string LITERALS (the common `if c then "a" else "b"`); other arm kinds fall
    /// back to today's sound Opaque form. Returns the result `dst`, or `None` (rolled
    /// back) when out of subset. Arms may be string LITERALS or a NESTED heap-result `if`
    /// (the else-if chain a desugared `match` produces — `match n { 0 => "a", _ => "b" }`),
    /// recursively. Other arm kinds fall back to today's sound Opaque form.
    pub(crate) fn try_lower_heap_result_if(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !is_heap_ty(result_ty) {
            return None;
        }
        // The whole attempt rolls back as a unit: the recursion below truncates nothing,
        // so the OUTERMOST call restores the op stream AND the live-handle set on any
        // out-of-subset arm (a call arm may have materialized + tracked a temp).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let result = self.lower_heap_result_if_inner(cond, then, else_, result_ty);
        if result.is_none() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        result
    }

    /// Materialize the CONDITION of a heap-result `if` to a scalar (Bool = i64 0/1)
    /// BEFORE the `IfThen` marker. The common shape is a pure `lower_scalar_value` cond
    /// (a comparison, a Var, a literal) — tried first, no ownership. When that defers, a
    /// Bool/Int-returning PURE call WITH HEAP ARGS (`if string.contains(s, x) then …`,
    /// `if list.is_empty(xs) then …`) is materialized via `try_lower_scalar_call`: the
    /// call's heap arg temps are pushed to `live_heap_handles`, and a per-cond frame
    /// (`drop_arm_locals`) frees them IMMEDIATELY after the call — they are transient to
    /// the condition, not owned by either arm. The scalar result is not a heap handle, so
    /// it survives the frame teardown. SOUND: the cond eval is internally balanced (each
    /// arg temp alloc'd `i` + dropped `d` within the frame), exactly the per-arm
    /// discipline; outside the pure-scalar-call subset it walls (`None` → Opaque). The
    /// gate keeping a heap-arg call OUT of `lower_scalar_value` (its rollback-safe, no-
    /// ownership contract) does not bind here — this position freely emits ownership ops.
    fn lower_heap_result_cond(&mut self, cond: &IrExpr) -> Option<ValueId> {
        // A scalar cond can itself MATERIALIZE a transient heap temp — `if c == "#"` lowers to
        // `string.eq(c, "#")` whose `"#"` literal is a fresh owned String. That temp is dead the
        // instant the Bool is computed, so it MUST be freed HERE, within a cond-local frame, BEFORE
        // the `IfThen` marker — never deferred to the enclosing arm's `drop_arm_locals`. The cond
        // of a NESTED heap-result `if` (the else-of-an-else parser shape) sits inside one arm's
        // wasm branch; deferring its temp's `Drop` to the OUTER block scope emits an UNCONDITIONAL
        // `rc_dec` of a local that the sibling arm never initialized (garbage/0 → trap). The frame
        // keeps the cond internally `i…d`-balanced exactly where it executes.
        let frame = self.live_heap_handles.len();
        if let Some(v) = self.lower_scalar_value(cond) {
            self.drop_arm_locals(frame);
            return Some(v);
        }
        // A scalar-returning (Bool/Int) PURE call with heap args — materialize it, then
        // free the transient arg temps within a cond-local frame.
        if let IrExprKind::Call { .. } = &cond.kind {
            if !is_heap_ty(&cond.ty) {
                if let Some(v) = self.try_lower_scalar_call(cond, &cond.ty) {
                    self.drop_arm_locals(frame);
                    return Some(v);
                }
            }
        }
        // A Bool-returning `==`/`!=` over HEAP operands neither of which is a tracked Var
        // (`list.first(xs) == some("")`, `string.first(s) == some(c)` — the toml `emit_sections`
        // blocker). `lower_scalar_value` above already covers the case where BOTH operands are
        // materialized Vars (its Option/Result eq reads them via `materialized_option_handle`);
        // here the operands are a heap-returning CALL and/or an Option/Result CTOR, which it
        // declines. Materialize EACH operand into an owned block in the SAME cond-local frame, run
        // the typed eq over their handles, then `drop_arm_locals(frame)` frees the operand temps —
        // they are transient to the condition, owned by NEITHER arm (the per-cond `i…d` balance,
        // exactly the pure-call-cond path's discipline). A non-materializable operand returns None
        // → the `if` keeps its sound Opaque wall (no invalid wasm, no leak).
        if let IrExprKind::BinOp { op: op @ (almide_ir::BinOp::Eq | almide_ir::BinOp::Neq), left, right } = &cond.kind {
            if matches!(cond.ty, Ty::Bool) && is_heap_ty(&left.ty) {
                if let Some(v) = self.lower_heap_eq_cond(left, right, matches!(op, almide_ir::BinOp::Neq)) {
                    self.drop_arm_locals(frame);
                    return Some(v);
                }
                // On decline, the partial ops/handles a half-materialized operand left are restored
                // by the OUTER `try_lower_heap_result_if` rollback (ops + live_heap_handles truncated
                // to its marks on any `None`), so the caller's Opaque fallback starts clean — no
                // extra teardown needed here.
            }
        }
        None
    }

    /// Lower a HEAP `left == right` / `left != right` cond (Bool) whose operands are NOT both
    /// tracked Vars — by MATERIALIZING each operand into an owned block (tracked in the cond
    /// frame so `drop_arm_locals` frees it after the compare) and running the typed eq over the
    /// two block handles. Reuses the SAME eq cores the Var-operand path uses
    /// (`option_scalar_eq_from_handles` / `option_heap_eq_from_handles` /
    /// `result_scalar_eq_from_handles`, `string.eq`, `value.eq`, `list.eq_*`) — no new eq op.
    /// `negate` wraps the result as `1 - eq` (for `!=`). SELF-CONTAINED ROLLBACK: on any decline a
    /// half-materialized operand may have emitted ops + pushed an owned temp to `live_heap_handles`;
    /// this restores BOTH to the pre-attempt marks so the method leaves NO trace on `None` (the
    /// rollback-safe contract the cond position requires — `lower_heap_result_cond` is reached from
    /// paths that do NOT all wrap it in an outer truncate, e.g. the defunc HOF body). Returns None
    /// (the caller walls) for an operand shape we cannot materialize, or a type whose eq is unhandled.
    fn lower_heap_eq_cond(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let ty = &left.ty;
        let eq = match self.lower_heap_eq_typed_materialized(left, right, ty) {
            Some(eq) => eq,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        if !negate {
            return Some(eq);
        }
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::Sub, a: one, b: eq });
        Some(dst)
    }

    /// Materialize both operands of a heap `==` into owned blocks (in the current cond frame) and
    /// emit the typed equality, returning the Bool ValueId. Handled operand TYPES: String,
    /// Value, List[scalar|Value], Option[scalar|heap], Result[scalar, String]. Each operand is
    /// materialized by `materialize_eq_operand` (a tracked Var is BORROWED, a fresh heap value is
    /// an owned temp added to `live_heap_handles` with its recursive drop set). The eq BORROWS the
    /// operand handles (it only reads), so the owned temps survive to the frame teardown.
    fn lower_heap_eq_typed_materialized(
        &mut self,
        left: &IrExpr,
        right: &IrExpr,
        ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        // String / Value / List[T] — a borrowed-handle module eq call. Reuse the same callee the
        // Var path uses; the operands are materialized into owned blocks the call borrows.
        let module_eq: Option<(&str, &str)> = if matches!(ty, Ty::String) {
            Some(("string", "eq"))
        } else if crate::lower::is_value_ty(ty) {
            Some(("value", "eq"))
        } else if let Ty::Applied(TC::List, es) = ty {
            if es.len() == 1 {
                if matches!(es[0], Ty::Int) {
                    Some(("list", "eq_int"))
                } else if matches!(es[0], Ty::String) {
                    Some(("list", "eq_str"))
                } else if matches!(es[0], Ty::Float) {
                    Some(("list", "eq_float"))
                } else if matches!(es[0], Ty::Bool) {
                    Some(("list", "eq_bool"))
                } else if crate::lower::is_value_ty(&es[0]) {
                    Some(("list", "eq_value"))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some((m, f)) = module_eq {
            let lb = self.materialize_eq_operand(left, ty)?;
            let rb = self.materialize_eq_operand(right, ty)?;
            let lh = self.handle_of(lb);
            let rh = self.handle_of(rb);
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: format!("{m}.{f}"),
                args: vec![CallArg::Handle(lh), CallArg::Handle(rh)],
                result: Some(repr_of(&Ty::Bool).ok()?),
            });
            return Some(dst);
        }
        // Option[T] — the scalar masked compare or the heap conditional compare, over the two
        // materialized DynListStr / OptSome blocks (read via their handles).
        if let Ty::Applied(TC::Option, oa) = ty {
            if oa.len() == 1 {
                let lb = self.materialize_eq_operand(left, ty)?;
                let rb = self.materialize_eq_operand(right, ty)?;
                let lh = self.handle_of(lb);
                let rh = self.handle_of(rb);
                if !is_heap_ty(&oa[0]) {
                    return Some(self.option_scalar_eq_from_handles(lh, rh, &oa[0]));
                }
                let eq_name: Option<&str> = if matches!(oa[0], Ty::String) {
                    Some("string.eq")
                } else if crate::lower::is_value_ty(&oa[0]) {
                    Some("value.eq")
                } else if let Ty::Applied(TC::List, es) = &oa[0] {
                    if es.len() == 1 && matches!(es[0], Ty::Int) {
                        Some("list.eq_int")
                    } else if es.len() == 1 && matches!(es[0], Ty::String) {
                        Some("list.eq_str")
                    } else if es.len() == 1 && matches!(es[0], Ty::Float) {
                        Some("list.eq_float")
                    } else if es.len() == 1 && matches!(es[0], Ty::Bool) {
                        Some("list.eq_bool")
                    } else if es.len() == 1 && crate::lower::is_value_ty(&es[0]) {
                        Some("list.eq_value")
                    } else {
                        None
                    }
                } else {
                    None
                };
                return self.option_heap_eq_from_handles(lh, rh, eq_name?);
            }
        }
        // Result[scalar, String] — the scalar/heap masked+conditional compare over the two
        // materialized DynListStr Result blocks.
        if let Ty::Applied(TC::Result, ra) = ty {
            if ra.len() == 2 && !is_heap_ty(&ra[0]) && matches!(ra[1], Ty::String) {
                let lb = self.materialize_eq_operand(left, ty)?;
                let rb = self.materialize_eq_operand(right, ty)?;
                let lh = self.handle_of(lb);
                let rh = self.handle_of(rb);
                return self.result_scalar_eq_from_handles(lh, rh, &ra[0]);
            }
        }
        None
    }

    /// The byte-address (`Prim::Handle`) of a materialized block — the operand handed to an eq core.
    fn handle_of(&mut self, block: ValueId) -> ValueId {
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![block] });
        h
    }

    /// Materialize ONE operand of a heap `==` cond into a block whose handle the eq core reads.
    /// - A tracked heap `Var` is BORROWED: its existing block is returned, NOT added to the cond
    ///   frame (it is owned elsewhere and drops at its own scope — the eq only reads it).
    /// - Any other heap operand (a heap-returning CALL, an Option/Result CTOR, a String literal /
    ///   concat) is materialized into a FRESH OWNED block via `lower_owned_heap_field`, which
    ///   pushes it to `live_heap_handles`; we then register its RECURSIVE drop set
    ///   (`heap_elem_lists` / `value_handles` / …) so the cond-frame `drop_arm_locals` frees it
    ///   AND its owned payload (no leak). The owned temp is freed exactly once (the frame `d`),
    ///   never double-freed (the eq borrows it, never consumes). Returns None for a non-
    ///   materializable shape → the caller walls.
    fn materialize_eq_operand(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &expr.kind {
            // A tracked heap Var (a param or let-local) — borrow its block, no new ownership.
            return self.value_for(*id).ok();
        }
        // An Option/Result CTOR (`some("")`, `none`, `ok(v)`, `err(m)`) — `try_lower_option_ctor`
        // (which handles BOTH Option and Result ctors) builds the DynListStr/OptSome block +
        // registers its recursive drop set (`materialize_opt_str_some` → `heap_elem_lists` +
        // `materialized_options`), but does NOT push to `live_heap_handles`. Push it so the cond-
        // frame `drop_arm_locals` frees it (and its owned payload) exactly once after the
        // (borrowing) eq.
        if matches!(
            &expr.kind,
            IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
        ) {
            let obj = self.try_lower_option_ctor(expr, ty)?;
            if !self.live_heap_handles.contains(&obj) {
                self.live_heap_handles.push(obj);
            }
            return Some(obj);
        }
        // Otherwise a heap-returning CALL / literal / concat — materialize a fresh OWNED block
        // (`lower_owned_heap_field` pushes it to `live_heap_handles`). The call path leaves the
        // block FLAT, so register the recursive drop set from the operand TYPE — else an
        // `Option[String]`/`List[String]` temp leaks its inner Strings. Idempotent.
        let obj = self.lower_owned_heap_field(expr)?;
        if self.live_heap_handles.contains(&obj) {
            self.register_owned_heap_eq_drop(obj, ty);
        }
        Some(obj)
    }

    /// Register the recursive drop set for a freshly materialized heap eq-operand block, mirroring
    /// the call-binding tracking in `lower_bind` so the cond-frame teardown frees nested ownership.
    fn register_owned_heap_eq_drop(&mut self, obj: ValueId, ty: &Ty) {
        if crate::lower::is_list_list_str_ty(ty) {
            self.list_list_str_lists.insert(obj);
        } else if crate::lower::is_list_str_str_ty(ty) {
            self.str_str_elem_lists.insert(obj);
        } else if crate::lower::is_list_int_str_ty(ty) {
            self.variant_drop_handles.insert(obj, "list_int_str".to_string());
        } else if crate::lower::is_lenlist_list_ty(ty) {
            self.variant_drop_handles.insert(obj, "list_lenlist".to_string());
        } else if is_heap_elem_list_ty(ty) {
            // List[heap] / Option[heap] / Result[_, heap] — the DynListStr recursive free.
            self.heap_elem_lists.insert(obj);
        }
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(obj);
        }
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_heap_result_cond(cond)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
        // RELEASE PARITY across the arms. An arm may MOVE an OUTER scope-level
        // handle into its result — the effect tail's `err(msg)` moves the error
        // accumulator into the Err block — which removes it from
        // `live_heap_handles` GLOBALLY, though the move runs only on that arm's
        // PATH. The sibling arm must then release it ITSELF: without the
        // compensating Drop the non-moving path LEAKS the handle (one error
        // accumulator per call on the happy path). The flat certificate hid this
        // by counting the moving arm's `m` unconditionally; the branch-grouped
        // cert (`{m|}` — arms disagree) REJECTS it, which is how it was found.
        // With the Drop the arms agree (`{m|d}`) and the leak is gone. Nested
        // heap-result ifs recurse through here, so parity holds level by level.
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let then_obj = self.lower_heap_result_arm(then, result_ty)?;
        let consumed_by_then: Vec<ValueId> =
            outer.iter().copied().filter(|h| !self.live_heap_handles.contains(h)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_obj) });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let else_obj = self.lower_heap_result_arm(else_, result_ty)?;
        let consumed_by_else: Vec<ValueId> = live_after_then
            .iter()
            .copied()
            .filter(|h| !self.live_heap_handles.contains(h))
            .collect();
        for h in &consumed_by_then {
            if !consumed_by_else.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.push(op); // the ELSE arm releases what THEN moved out
            }
        }
        for h in &consumed_by_else {
            if !consumed_by_then.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.insert(else_marker_at, op); // the THEN arm releases what ELSE moved out
            }
        }
        self.ops.push(Op::EndIf { val: Some(else_obj) });
        Some(dst)
    }
}
