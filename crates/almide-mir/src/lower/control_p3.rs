/// The outcome of one `??` lowering ROUTE attempt inside
/// [`LowerCtx::try_lower_option_unwrap_or`].
///
/// The routes are mutually exclusive `??` shapes tried IN ORDER (heap-Value
/// helper call, variant-ctor fallback, closure fallback, String fallback,
/// scalar); the split preserves the original single-function control flow
/// exactly: `Done` was `return Some(..)`, `Next` was falling through to the
/// next shape check (nothing emitted yet by the route), and `Fail` was the
/// rollback-and-`return None` tail — the route has ALREADY truncated the
/// ops/handle marks back when it reports `Fail`, so the caller just declines.
enum UnwrapOrRoute {
    /// The route matched and lowered — carries the `??` result value.
    Done(ValueId),
    /// Not this route's shape — try the next one.
    Next,
    /// The route matched but could not lower; ops are rolled back and the
    /// whole `??` declines.
    Fail,
}

impl UnwrapOrRoute {
    /// Chain to the next route only on `Next` — `Done`/`Fail` short-circuit,
    /// exactly as the former inline `return Some(..)` / `return None` did.
    fn or_try(self, next: impl FnOnce() -> UnwrapOrRoute) -> UnwrapOrRoute {
        match self {
            UnwrapOrRoute::Next => next(),
            decided => decided,
        }
    }
}

/// The per-`??` state every route shares, taken once at
/// [`LowerCtx::try_lower_option_unwrap_or`] entry.
#[derive(Copy, Clone)]
struct UnwrapOrCtx {
    /// Read the operand tag INVERSELY (a Result: Ok = `tag == 0`) — see
    /// [`LowerCtx::unwrap_or_is_result`].
    is_result: bool,
    /// `true` (a let-bind / call-arg temp): push the fresh owned heap result to
    /// `live_heap_handles` for the scope-end drop; `false` (a RETURN/tail
    /// position): leave it untracked — it is MOVED OUT to the caller.
    track_result: bool,
    /// Op-stream rollback mark (`ops.len()` at entry).
    ops_mark: usize,
    /// Scope-end drop-set rollback mark (`live_heap_handles.len()` at entry).
    lhh_mark: usize,
}

impl LowerCtx {
    /// Which self-host `??` unwrap-or helper handles a HEAP-Value Result/Option `??` payload
    /// type, keyed purely by `expr_ty` — a pure classification (no ownership/rollback state,
    /// unlike the rest of [`Self::try_lower_option_unwrap_or`]). Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn value_unwrap_or_helper_name(expr_ty: &Ty) -> Option<&'static str> {
        if crate::lower::is_result_listval_ty(expr_ty) {
            return Some("result.list_value_unwrap_or");
        }
        if crate::lower::is_value_result_ty(expr_ty) {
            return Some("result.value_unwrap_or");
        }
        if crate::lower::is_result_str_str_ty(expr_ty) {
            return Some("result.str_unwrap_or");
        }
        if crate::lower::is_option_value_ty(expr_ty) {
            return Some("option.value_unwrap_or");
        }
        if crate::lower::is_option_liststr_ty(expr_ty) {
            return Some("option.liststr_unwrap_or");
        }
        if crate::lower::is_option_listscalar_ty(expr_ty) {
            // `map.get(groups, k) ?? []` — Option[List[<scalar>]] (the group_by class):
            // the FLAT sibling (scalar elements own nothing; flat rc drop is exact).
            return Some("option.listint_unwrap_or");
        }
        if crate::lower::is_option_listvalue_ty(expr_ty) {
            return Some("option.listvalue_unwrap_or");
        }
        // An `Option[record]` payload (`list.get(tools, i) ?? { name: "", … }`) is NOT
        // routed to `option.value_unwrap_or`: that helper does a VALUE-shaped handle select
        // (it reads a tagged Value block), which CORRUPTS a plain record field block — BOTH
        // the Some and the None arm printed garbage (0x18 0x20…) vs v0's real field (a
        // pre-existing miscompile the mir>ir gate flagged on porta parse_manifest). Decline
        // here so the `??` walls cleanly; a correct record-payload unwrap-or is a follow-up.
        None
    }

    /// Roll the op stream and the scope-end drop set back to their marks — the
    /// shared decline tail every `??` route uses before reporting failure.
    fn unwrap_or_rollback(&mut self, cx: UnwrapOrCtx) {
        self.ops.truncate(cx.ops_mark);
        self.live_heap_handles.truncate(cx.lhh_mark);
    }

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
    ///
    /// This is the route DISPATCHER: operand classification/resolution first, then the
    /// mutually exclusive `??` shapes in their original order — heap-Value helper,
    /// variant-ctor fallback, closure fallback, String fallback, scalar. Each route's
    /// [`UnwrapOrRoute`] maps 1:1 onto the former inline control flow (see the enum doc),
    /// so this split is behavior-preserving — docs/roadmap/active/code-health-codopsy.md.
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
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
        // Shared with the caps counter (`unwrap_or_named_variant_operand`) so the gate that
        // ADMITS a user-fn operand here and the gate that CREDITS its synthetic helper call in
        // `count_ir_calls` can never drift apart (#1079).
        let is_named_variant_call = unwrap_or_named_variant_operand(expr);
        let cx = UnwrapOrCtx {
            is_result: self.unwrap_or_is_result(expr, is_named_variant_call),
            track_result,
            ops_mark: self.ops.len(),
            lhh_mark: self.live_heap_handles.len(),
        };
        let handle = self.unwrap_or_operand_handle(expr, is_named_variant_call, cx)?;
        let route = self
            .lower_value_helper_unwrap_or(handle, expr, fallback, cx)
            .or_try(|| self.lower_variant_ctor_unwrap_or(handle, expr, fallback, cx))
            .or_try(|| self.lower_closure_unwrap_or(handle, expr, fallback, cx))
            .or_try(|| self.lower_str_unwrap_or(handle, fallback, cx));
        match route {
            UnwrapOrRoute::Done(v) => Some(v),
            UnwrapOrRoute::Fail => None,
            UnwrapOrRoute::Next => {
                // TERMINAL TYPE GUARD (the route chain's safety floor): the scalar
                // route blind-reads len@4 + slot 0 — valid ONLY for a genuinely
                // len-as-tag-readable operand. Every other type that slips past
                // the routes above (an admitted-but-unroutable heap-Ok Result, a
                // future admission-list mistake) must DECLINE to an honest wall
                // here, never fall into the misread (a cap-as-tag block has
                // len@4 = 1 on BOTH arms — the silent wrong-branch class).
                if !scalar_route_len_tag_readable(&expr.ty) {
                    self.unwrap_or_rollback(cx);
                    return None;
                }
                self.lower_scalar_unwrap_or(handle, fallback, cx)
            }
        }
    }

    /// Is this `??` operand a RESULT (read its tag INVERSELY) rather than an Option?
    ///
    /// A `??` operand is EITHER an Option (`o ?? d` → Some-payload / fallback) OR a scalar Result
    /// (`int.parse(s) ?? -1` → Ok-payload / fallback). They share the len-as-tag layout but read
    /// INVERSELY: Option Some = `tag != 0` (take payload), Result Ok = `tag == 0` (take payload).
    /// The answer selects the arm arrangement in the scalar route; a Result operand also skips the
    /// Option-only `option.unwrap_or_str` String branch (a `Result[String,String] ?? "d"` is a
    /// later case).
    fn unwrap_or_is_result(&self, expr: &IrExpr, is_named_variant_call: bool) -> bool {
        match &expr.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .ok()
                .map(|v| {
                    self.materialized_results.contains(&v)
                        && !self.materialized_options.contains(&v)
                })
                .unwrap_or(false),
            // A Result-typed FIELD/TUPLE-SLOT (`h.r ?? -1` / `t.0 ?? -1` — C-068):
            // the borrowed block reads its tag INVERSELY like any Result.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } => is_result_ty(&expr.ty),
            // A USER function returning Result — read its tag INVERSELY (Ok = tag 0).
            _ if is_named_variant_call => is_result_ty(&expr.ty),
            // A FALLIBLE-CLOSURE call (`g("21") ?? -1` — the ADR-0009 carrier): the
            // call's declared type IS the carrier, so a Result carrier reads INVERSELY.
            // Without this arm a closure-call Result operand fell to the Option read
            // and would have taken the wrong tag sense.
            IrExprKind::Call { target: CallTarget::Computed { .. }, .. } => {
                is_result_ty(&expr.ty)
            }
            // A str-family `Result[Unit, String]` call (fs.write/fs.copy — the moved
            // ctor family, result-family-from-type Phase 1) reads INVERSELY too: its
            // block carries a valid len-as-tag reading (Ok len 0 / Err len 1), so the
            // scalar `??` route below is exact for it.
            _ => {
                is_self_host_result_call(expr)
                    || (is_self_host_result_str_call(expr)
                        && crate::lower::is_result_unit_str_ty(&expr.ty))
            }
        }
    }

    /// Resolve the `??` OPERAND to its Option/Result block handle, or `None` when the
    /// operand is out of subset (the whole `??` declines; any partially emitted ops were
    /// already rolled back by the failing arm).
    ///
    /// The Option operand's handle: either a VAR already bound to a materialized Option
    /// (`let o = list.get(xs, i); o ?? d` — the most common form, BORROWED, dropped by its
    /// own let-bind at scope end), a function PARAM of Option type (`fn f(o: Option[String]) =
    /// o ?? d` — passed by the caller as a real materialized Option block, BORROWED, not dropped
    /// here), or a DIRECT self-host Option call (materialized here). The param case is sound by
    /// the same evidence as `materialized_options`: an Option-typed param is a real 0-or-1-
    /// element block (the calling convention), so its len-as-tag read is correct — NOT a deferred
    /// Opaque (those are never params), which is why the bare-Var gate excludes non-Option Vars.
    fn unwrap_or_operand_handle(
        &mut self,
        expr: &IrExpr,
        is_named_variant_call: bool,
        cx: UnwrapOrCtx,
    ) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &expr.kind {
            return self.var_unwrap_or_operand(*id, &expr.ty);
        }
        if matches!(&expr.kind, IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }) {
            return self.field_unwrap_or_operand(expr);
        }
        // A NESTED `??` operand (`(list.find(..) ?? none) ?? -1` — the fallible-HOF
        // find shape, #1134 Shape 2): ANF it — bind the INNER `??` to a synthetic
        // temp through the ordinary bind machinery (which owns + scope-tracks it and
        // seeds its variant read shape), then resolve the temp exactly like a
        // user-written two-step `let inner = ..; inner ?? d`. The bind declining
        // rolls back to None (the wall stays honest).
        if matches!(&expr.kind, IrExprKind::UnwrapOr { .. }) {
            let tmp = self.fresh_synth_var();
            if self.lower_bind(tmp, &expr.ty, expr).is_err() {
                self.unwrap_or_rollback(cx);
                return None;
            }
            let Ok(v) = self.value_for(tmp) else {
                self.unwrap_or_rollback(cx);
                return None;
            };
            return self.var_unwrap_or_operand_value(v, &expr.ty);
        }
        if is_self_host_option_call(expr)
            || is_self_host_result_call(expr)
            || (is_self_host_result_str_call(expr) && unwrap_or_heap_ok_route_exists(&expr.ty))
            || is_named_variant_call
        {
            return self.call_unwrap_or_operand(expr, cx);
        }
        // A FALLIBLE-CLOSURE call operand (`g("21") ?? -1` where `g = (x) => parse1(x)! * 2`
        // — the ADR-0009 first-class fallible lambda, #1134 Shape 2): dispatch through the
        // tracked closure block exactly like the bind-position Computed arm — the call
        // returns the fresh OWNED len-as-tag carrier block, tracked + type-routed for the
        // scope-end drop like any owned call temp, and seeded as a materialized read shape
        // so the downstream tag read is over a KNOWN layout. Gated to a SCALAR-payload
        // Option/Result carrier: the scalar route reads tag + payload with no ownership
        // transfer (a heap-payload carrier stays walled — the recursive-ownership frontier).
        if let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &expr.kind {
            use almide_lang::types::constructor::TypeConstructorId as TC;
            let scalar_opt = matches!(&expr.ty,
                Ty::Applied(TC::Option, a) if a.len() == 1 && !is_heap_ty(&a[0]));
            let scalar_res = matches!(&expr.ty,
                Ty::Applied(TC::Result, a)
                    if a.len() == 2 && !is_heap_ty(&a[0]) && matches!(a[1], Ty::String));
            if !scalar_opt && !scalar_res {
                return None;
            }
            let route = (|s: &mut Self| {
                let blk = s.closure_block_of_mut(callee)?;
                let repr = repr_of(&expr.ty).ok()?;
                let lowered = s.lower_call_args(args).ok()?;
                let obj = s.fresh_value();
                s.emit_closure_call(blk, Some(obj), lowered, Some(repr));
                s.live_heap_handles.push(obj);
                s.register_owned_heap_eq_drop(obj, &expr.ty);
                if scalar_res {
                    s.materialized_results.insert(obj);
                } else {
                    s.materialized_options.insert(obj);
                }
                Some(obj)
            })(self);
            if route.is_none() {
                self.unwrap_or_rollback(cx);
            }
            return route;
        }
        // A `??` over a variant-returning call NOT in the self-host registries — the
        // `json.parse(s) ?? d` (PURE heap-Result) / `process.env(k) ?? d` (IMPURE intrinsic
        // Option[String]) class. `materialize_unwrap_or_operand` routes it through the SAME
        // proven machinery a recognized self-host operand uses (the recursive drop is
        // registered by type), so the helper read downstream (`option.unwrap_or_str` /
        // `result.value_unwrap_or`) is over a real owned block, freed once at scope end.
        self.materialize_unwrap_or_operand(expr)
    }

    /// A bare-Var `??` operand: admit only a tracked materialized Option/Result OR a
    /// borrowed variant PARAM (`param_values` — same calling-convention soundness as the
    /// match): a deferred Opaque Var (len 0) would MISREAD as None/Err, so it is excluded.
    fn var_unwrap_or_operand(&self, id: VarId, expr_ty: &Ty) -> Option<ValueId> {
        let v = self.value_for(id).ok()?;
        self.var_unwrap_or_operand_value(v, expr_ty)
    }

    /// The admission half of [`Self::var_unwrap_or_operand`], keyed on the resolved
    /// `ValueId` — shared with the nested-`??` ANF route, whose synthetic temp has a
    /// value but no user-facing var lookup to repeat.
    fn var_unwrap_or_operand_value(&self, v: ValueId, expr_ty: &Ty) -> Option<ValueId> {
        let admitted = self.materialized_options.contains(&v)
            || self.materialized_results.contains(&v)
            // a Value/List-Ok Result Var (`value.get`/`value.as_array` result) — its `??`
            // routes to the value_unwrap helper route. A String-Ok Result (heap_elem_lists)
            // is NOT admitted here: it keeps its original path (the String branch is for
            // OPTION[String], and counting a str-Result there falsely taints mir>ir).
            || self.value_result_results.contains(&v)
            || self.value_result_lists.contains(&v)
            // A `Result[String, String]` Var (cap-as-tag, materialized_results_str)
            // routes to the `result.str_unwrap_or` helper route — admitted ONLY for
            // that type (any other _str-set shape would mis-take the len-as-tag
            // String branch, reading an Err payload as Some).
            || (self.materialized_results_str.contains(&v)
                && crate::lower::is_result_str_str_ty(expr_ty))
            || self.param_values.contains(&v);
        admitted.then_some(v)
    }

    /// `r.opt ?? d` — an `Option[scalar/String]` FIELD of a materialized record: BORROW the
    /// field's Option block handle (`LoadHandle` at the field offset; the record keeps
    /// ownership, the `??` only READS the tag + scalar/String payload — no transfer, no drop).
    /// Gated to a scalar/String Option leaf so the scalar-payload / `option.unwrap_or_str` read
    /// downstream is over a real 0-or-1 block. Exposed by derived-Codec Option decode (the
    /// codec_float_int `r.opt ?? -1.0` consumer): without it the `??` fell to a silent Const-0.
    /// A `Result[scalar, _]` field/tuple-slot (`h.r ?? -1` / `t.0 ?? -1` — the
    /// C-068 autotry construction class) is the SAME len-as-tag block, read
    /// INVERSELY via `unwrap_or_is_result`.
    fn field_unwrap_or_operand(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        let leaf_ok = matches!(&expr.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a)
                if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Float | Ty::Bool | Ty::String))
            || matches!(&expr.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                    if a.len() == 2 && matches!(a[0], Ty::Int | Ty::Float | Ty::Bool));
        if !leaf_ok {
            return None;
        }
        let (object, offset) = match &expr.kind {
            IrExprKind::Member { object, field } => {
                (object, self.aggregate_field_offset_any(&object.ty, field.as_str())?)
            }
            IrExprKind::TupleIndex { object, index } => {
                (object, self.aggregate_index_offset_any(&object.ty, *index)?)
            }
            _ => unreachable!(),
        };
        let ch = match self.resolve_aggregate_container_handle(object) {
            Some(c) => c,
            None => return None,
        };
        Some(self.load_at_offset(ch, offset as i64, PrimKind::LoadHandle))
    }

    /// A self-host OR user-function call returning Option/Result — materialize it (the
    /// Named-call arm seeds the READ-shape into `materialized_options/results`, so the
    /// tag read downstream is over a KNOWN-layout block) and read its tag, exactly like a
    /// tracked Var. The owned result is dropped at scope end by `materialized_call_arg`.
    fn call_unwrap_or_operand(&mut self, expr: &IrExpr, cx: UnwrapOrCtx) -> Option<ValueId> {
        match self.lower_call_args(std::slice::from_ref(expr)) {
            Ok(args) => match args.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => {
                    self.unwrap_or_rollback(cx);
                    None
                }
            },
            Err(_) => {
                self.unwrap_or_rollback(cx);
                None
            }
        }
    }

    /// HEAP-Value Result `??` (`value.get(o,k) ?? value.null()` — Result[Value,String]): route to
    /// the self-hosted `result.value_unwrap_or`, which reuses the Value-Ok match read (its
    /// `ok(v) => v` arm Dup's the @12 payload; the Result is freed by its scope-end DropResultValue).
    /// A call returning a FRESH owned Value sidesteps the bind-position heap-result rc bookkeeping,
    /// exactly like `option.unwrap_or_str` for the String payload.
    /// The Ok payload selects the helper: a single Value (`value.get`) → `result.value_unwrap_or`;
    /// a `List[Value]` (`value.as_array`) → `result.list_value_unwrap_or` (recursive list drop).
    /// Both reuse the working heap-Ok match read; a call returning a fresh owned heap value
    /// sidesteps the bind-position rc bookkeeping, like `option.unwrap_or_str` for the String case.
    fn lower_value_helper_unwrap_or(
        &mut self,
        handle: ValueId,
        expr: &IrExpr,
        fallback: &IrExpr,
        cx: UnwrapOrCtx,
    ) -> UnwrapOrRoute {
        let Some(helper) = Self::value_unwrap_or_helper_name(&expr.ty) else {
            return UnwrapOrRoute::Next;
        };
        if !is_heap_ty(&fallback.ty) {
            return UnwrapOrRoute::Next;
        }
        let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
            Ok(a) => a,
            Err(_) => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
            }
        };
        let repr = match repr_of(&expr.ty) {
            Ok(r) => r,
            Err(_) => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
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
        if cx.track_result {
            self.live_heap_handles.push(dst);
        }
        UnwrapOrRoute::Done(dst)
    }

    /// `Option[<custom variant>] ?? <ctor(...)>` (`list.get(xs, i) ?? Empty` — a
    /// custom-ADT element list's search-with-fallback shape, `variant_ctor_fn_test`):
    /// Some → BORROW the payload handle (LoadHandle @12 — the Option/its source list
    /// keeps owning it) then `Dup` to a fresh OWNED reference (the SAME borrowed-param
    /// Some(p) precedent used throughout this file); None → build the fallback via
    /// `try_lower_variant_ctor` (already a fresh owned value, no Dup needed). Both arms
    /// now produce UNIFORM (owned) values, merged via the proven `Op::IfThen`/`Else`/
    /// `EndIf` heap-result-if skeleton (the SAME shape the scalar route uses for
    /// scalar payloads) — never the scalar `Load{width:8}` fallback of the scalar route,
    /// which would misread the payload HANDLE as a raw i64 scalar.
    /// Any unmet guard reports `Next` (the NEXT `??` shape is checked, exactly as the
    /// former guard-clause fall-through did).
    fn lower_variant_ctor_unwrap_or(
        &mut self,
        handle: ValueId,
        expr: &IrExpr,
        fallback: &IrExpr,
        cx: UnwrapOrCtx,
    ) -> UnwrapOrRoute {
        use crate::PrimKind;
        if cx.is_result {
            return UnwrapOrRoute::Next;
        }
        let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &expr.ty
        else {
            return UnwrapOrRoute::Next;
        };
        if a.len() != 1 {
            return UnwrapOrRoute::Next;
        }
        let Ty::Named(tn, _) = &a[0] else {
            return UnwrapOrRoute::Next;
        };
        if !self.variant_layouts.by_type.contains_key(tn.as_str()) {
            return UnwrapOrRoute::Next;
        }
        let is_ctor_fallback = matches!(
            &fallback.kind,
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str())
        ) || matches!(
            &fallback.kind,
            IrExprKind::Record { name: Some(n), .. }
                if self.variant_layouts.ctor_to_type.contains_key(n.as_str())
        );
        if !is_ctor_fallback {
            return UnwrapOrRoute::Next;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        let borrowed = self.load_at_offset(h, 12, PrimKind::LoadHandle);
        let owned = self.fresh_value();
        self.ops.push(Op::Dup { dst: owned, src: borrowed });
        self.ops.push(Op::Else { val: Some(owned) });
        match self.try_lower_variant_ctor(fallback) {
            Some(fb) => {
                self.ops.push(Op::EndIf { val: Some(fb) });
            }
            None => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
            }
        }
        if cx.track_result {
            self.live_heap_handles.push(result);
        }
        UnwrapOrRoute::Done(result)
    }

    /// `Option[<Fn>] ?? <lambda>` (`map.get(m, "add") ?? ((x) => x)` — the
    /// closure-valued map coalesce): Some → BORROW the payload handle @12 (the
    /// closure block — the Option keeps owning it) then `Dup` a fresh OWNED
    /// reference; None → LIFT the fallback lambda (a fresh owned closure block).
    /// Both arms merge through the SAME proven IfThen/Else/EndIf heap-result
    /// skeleton the variant-ctor fallback uses; the result joins
    /// `closure_values` (CallIndirect dispatch + the `$__drop_closure` route).
    fn lower_closure_unwrap_or(
        &mut self,
        handle: ValueId,
        expr: &IrExpr,
        fallback: &IrExpr,
        cx: UnwrapOrCtx,
    ) -> UnwrapOrRoute {
        use crate::PrimKind;
        if cx.is_result {
            return UnwrapOrRoute::Next;
        }
        let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &expr.ty
        else {
            return UnwrapOrRoute::Next;
        };
        if a.len() != 1 || !matches!(a[0], Ty::Fn { .. }) {
            return UnwrapOrRoute::Next;
        }
        let IrExprKind::Lambda { params, body, .. } = &fallback.kind else {
            return UnwrapOrRoute::Next;
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        let borrowed = self.load_at_offset(h, 12, PrimKind::LoadHandle);
        let owned = self.fresh_value();
        self.ops.push(Op::Dup { dst: owned, src: borrowed });
        self.ops.push(Op::Else { val: Some(owned) });
        let (params, body) = (params.clone(), body.clone());
        match self.lift_lambda(&params, &body) {
            Some(fb) => {
                // The fallback block is ELSE-ARM-LOCAL and MOVES into the
                // merge (`EndIf val`) — remove it from the scope-end drop
                // set (`lift_lambda` pushed it): an unconditional drop
                // would free a never-allocated local (0) on the Some path.
                self.live_heap_handles.retain(|x| *x != fb);
                self.ops.push(Op::EndIf { val: Some(fb) });
            }
            None => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
            }
        }
        if cx.track_result {
            self.live_heap_handles.push(result);
        }
        self.closure_values.insert(result);
        UnwrapOrRoute::Done(result)
    }

    /// HEAP-String result (`Option[String] ?? "default"` — the most common heap `??`): the scalar
    /// unwrap route can't carry a heap payload (it would mis-read the slot-0 String HANDLE as an
    /// i64 scalar). Route to the self-host `option.unwrap_or_str` CALL — a call returning a FRESH
    /// owned String (cert `i`, bound + dropped like any heap value), which sidesteps the
    /// bind-position heap-result-`if` cert problem entirely. The Option is BORROWED (the callee
    /// reads + copies it); the fallback is materialized/borrowed by `lower_call_args`. Gated to
    /// `Ty::String` (a `List`/other-heap payload would corrupt — its slot is not a String handle),
    /// and `count_ir_calls` counts a String-fallback `UnwrapOr` node so this synthetic call keeps
    /// `mir_calls <= ir_calls` (the same accounting as the `__str_concat` operator-desugar).
    fn lower_str_unwrap_or(
        &mut self,
        handle: ValueId,
        fallback: &IrExpr,
        cx: UnwrapOrCtx,
    ) -> UnwrapOrRoute {
        if !matches!(fallback.ty, Ty::String) || cx.is_result {
            return UnwrapOrRoute::Next;
        }
        let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
            Ok(a) => a,
            Err(_) => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
            }
        };
        let repr = match repr_of(&fallback.ty) {
            Ok(r) => r,
            Err(_) => {
                self.unwrap_or_rollback(cx);
                return UnwrapOrRoute::Fail;
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
        if cx.track_result {
            self.live_heap_handles.push(dst);
        }
        UnwrapOrRoute::Done(dst)
    }

    /// A SCALAR `??`: read the tag (len @4) and pick the slot-0 payload vs the fallback. The
    /// payload is an i64 value COPY (`Load width 8`) — fine for a scalar Ok/Some; a heap payload
    /// is handled by the String route (Option) or stays out of subset (Result[String,…]).
    fn lower_scalar_unwrap_or(
        &mut self,
        handle: ValueId,
        fallback: &IrExpr,
        cx: UnwrapOrCtx,
    ) -> Option<ValueId> {
        use crate::PrimKind;
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
        if cx.is_result {
            // THEN = Err (tag != 0) → the fallback computed HERE; ELSE = Ok → the slot-0 payload.
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.unwrap_or_rollback(cx);
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
                    self.unwrap_or_rollback(cx);
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
        // Every caller emits this arm under REAL `IfThen`/`Else`/`EndIf` markers (the
        // scalar-if machinery, the variant/refinement chains, the desugared match
        // chains) — exactly ONE arm runs at runtime, so a mutable-global/shared-cell
        // write inside it is a real conditional effect, and a scalar reassignment of
        // an outer var must hit its stable local (see `LowerCtx::unit_arm_depth`).
        // Without this the modeled-frame fence (`in_frame > 0 && unit_arm_depth == 0`)
        // misfired on the ordinary early-return guard `if len(d) < 12 then 0 else
        // { bytes.set_i32_le(g, …) … }`: the arm's stmt walled, the scalar-if rolled
        // back, and the whole fn fell to the linearization wall blaming the CONDITION
        // (#907, nendo's load_vrm_data). The fence still walls the true both-arms
        // linearization — `lower_branch_arm` raises only `in_frame`.
        self.unit_arm_depth += 1;
        for stmt in stmts {
            if self.lower_stmt(stmt).is_err() {
                self.unit_arm_depth -= 1;
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
            // A TUPLE-of-bound-vars subject (the frontend's factored form of a
            // multi-arm nested-ctor match — C-070): per-element refinement chain.
            IrExprKind::Match { subject, arms }
                if matches!(subject.kind, IrExprKind::Tuple { .. }) =>
            {
                self.try_lower_tuple_refinement_match(subject, arms, &t.ty)
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
        self.unit_arm_depth -= 1;
        self.in_frame -= 1;
        self.drop_arm_locals(lhh_mark);
        val
    }
}

/// Does the `??` machinery have an EXECUTABLE route for a heap-Ok (str-family)
/// Result operand of this TYPE? This is ROUTE CAPABILITY, not family: the
/// family (`result_family`) says which physical layout the block has; this
/// says whether one of the downstream routes (`lower_value_helper_unwrap_or` →
/// variant-ctor → closure → `lower_str_unwrap_or` → `lower_scalar_unwrap_or`)
/// can faithfully compute `block ?? fallback` for that payload class. A
/// heap-Ok type with NO route (e.g. `Result[List[Int], String] ?? [..]` — a
/// heap fallback the scalar route cannot build) must DECLINE so the wall stays
/// honest. Named and single-sourced so adding a route and admitting its type
/// happen in one place — the anonymous inline conjunction this replaces was
/// the same point-wise-list disease the result-family arc razed, one level up.
fn unwrap_or_heap_ok_route_exists(ty: &Ty) -> bool {
    // value / List[Value] payloads → the value-helper route;
    // String payload → the str route (`result.str_unwrap_or`);
    // Unit payload → the scalar route (the block also carries the len-as-tag
    // reading; the fallback `()` lowers as 0 — the Unit-is-0 convention).
    crate::lower::is_value_result_ty(ty)
        || crate::lower::is_result_listval_ty(ty)
        || crate::lower::is_result_str_str_ty(ty)
        || crate::lower::is_result_unit_str_ty(ty)
}

/// Is a blind len-as-tag read (len@4 as the tag, slot 0 as an i64-copy payload —
/// what `lower_scalar_unwrap_or` emits) VALID for this operand type? The
/// terminal guard of the `??` route chain: a scalar-family Result (len 0 = Ok /
/// len 1 = Err), `Result[Unit, String]` (one layout since the result-family
/// arc — both the len and the @16 read are valid), or a SCALAR-payload Option
/// (the 0-or-1-element block). A heap-Ok cap-as-tag Result has len@4 = 1 on
/// BOTH arms — reading it here is the silent wrong-branch class this guard
/// makes unrepresentable.
fn scalar_route_len_tag_readable(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Result, _) => {
            crate::lower::result_family(ty) == crate::lower::ResultFamily::Scalar
                || crate::lower::is_result_unit_str_ty(ty)
        }
        Ty::Applied(TC::Option, a) if a.len() == 1 => !is_heap_ty(&a[0]),
        _ => false,
    }
}
